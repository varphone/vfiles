use std::{io, net::SocketAddr, sync::Arc, time::Duration};

use axum::{Router, body::Body, extract::ConnectInfo, http::Request};
use hyper::body::Incoming;
use hyper_util::{
    rt::{TokioExecutor, TokioIo, TokioTimer},
    server::conn::auto::Builder,
    service::TowerToHyperService,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Semaphore, watch},
    task::JoinSet,
};
use tower::ServiceExt;

const MAX_ACTIVE_HTTP_CONNECTIONS: usize = 512;
const HTTP1_HEADER_READ_TIMEOUT: Duration = Duration::from_secs(15);
const HTTP2_MAX_CONCURRENT_STREAMS: u32 = 32;

pub async fn serve(
    listener: TcpListener,
    router: Router,
    shutdown: watch::Receiver<bool>,
) -> io::Result<()> {
    serve_with_limits(
        listener,
        router,
        shutdown,
        MAX_ACTIVE_HTTP_CONNECTIONS,
        HTTP1_HEADER_READ_TIMEOUT,
    )
    .await
}

async fn serve_with_limits(
    listener: TcpListener,
    router: Router,
    mut shutdown: watch::Receiver<bool>,
    max_connections: usize,
    header_read_timeout: Duration,
) -> io::Result<()> {
    let connection_permits = Arc::new(Semaphore::new(max_connections.max(1)));
    let mut connections = JoinSet::new();

    loop {
        if *shutdown.borrow() {
            break;
        }

        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, remote_addr)) => {
                        let permit = match Arc::clone(&connection_permits).try_acquire_owned() {
                            Ok(permit) => permit,
                            Err(_) => {
                                tracing::debug!(%remote_addr, max_connections, "HTTP connection limit reached; closing connection");
                                drop(stream);
                                continue;
                            }
                        };
                        configure_accepted_stream(&stream, remote_addr);
                        connections.spawn(serve_connection(
                            stream,
                            remote_addr,
                            router.clone(),
                            shutdown.clone(),
                            header_read_timeout,
                            permit,
                        ));
                    }
                    Err(error) => {
                        tracing::warn!(%error, "HTTP listener accept failed; retrying");
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
        }
    }

    while let Some(result) = connections.join_next().await {
        if let Err(error) = result {
            tracing::warn!(%error, "HTTP connection task did not stop cleanly");
        }
    }

    Ok(())
}

fn configure_accepted_stream(stream: &TcpStream, remote_addr: SocketAddr) {
    if let Err(error) = stream.set_nodelay(true) {
        tracing::warn!(%remote_addr, %error, "failed to enable TCP_NODELAY for HTTP connection");
    }
}

async fn serve_connection(
    stream: TcpStream,
    remote_addr: SocketAddr,
    router: Router,
    mut shutdown: watch::Receiver<bool>,
    header_read_timeout: Duration,
    _permit: tokio::sync::OwnedSemaphorePermit,
) {
    let service = TowerToHyperService::new(tower::service_fn(move |request: Request<Incoming>| {
        let router = router.clone();
        async move {
            let (mut parts, body) = request.into_parts();
            parts.extensions.insert(ConnectInfo(remote_addr));
            router
                .oneshot(Request::from_parts(parts, Body::new(body)))
                .await
        }
    }));
    let io = TokioIo::new(stream);
    let mut builder = Builder::new(TokioExecutor::new());
    builder
        .http1()
        .timer(TokioTimer::new())
        .header_read_timeout(header_read_timeout);
    builder
        .http2()
        .timer(TokioTimer::new())
        .enable_connect_protocol()
        .max_concurrent_streams(HTTP2_MAX_CONCURRENT_STREAMS);

    let mut connection = Box::pin(builder.serve_connection_with_upgrades(io, service));
    let result = tokio::select! {
        result = connection.as_mut() => result,
        _ = wait_for_shutdown(&mut shutdown) => {
            connection.as_mut().graceful_shutdown();
            connection.await
        }
    };

    if let Err(error) = result {
        tracing::debug!(%remote_addr, %error, "HTTP connection closed with an error");
    }
}

async fn wait_for_shutdown(shutdown: &mut watch::Receiver<bool>) {
    loop {
        if *shutdown.borrow() || shutdown.changed().await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, time::Duration};

    use axum::{Router, extract::ConnectInfo, routing::get};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpStream,
        sync::watch,
    };

    use super::{HTTP2_MAX_CONCURRENT_STREAMS, configure_accepted_stream, serve_with_limits};

    #[tokio::test]
    async fn accepted_streams_enable_tcp_nodelay() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let address = listener.local_addr().expect("test address should exist");
        let client = TcpStream::connect(address)
            .await
            .expect("client should connect");
        let (stream, _) = listener
            .accept()
            .await
            .expect("connection should be accepted");

        configure_accepted_stream(&stream, address);

        assert!(stream.nodelay().expect("TCP_NODELAY should be readable"));
        drop(client);
    }

    async fn start_server(
        max_connections: usize,
        header_read_timeout: Duration,
    ) -> (
        std::net::SocketAddr,
        watch::Sender<bool>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test listener should bind");
        let address = listener.local_addr().expect("test address should exist");
        let app = Router::new().route(
            "/",
            get(|ConnectInfo(address): ConnectInfo<SocketAddr>| async move {
                format!("ok {}", address.ip())
            }),
        );
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let server = tokio::spawn(async move {
            serve_with_limits(
                listener,
                app,
                shutdown_rx,
                max_connections,
                header_read_timeout,
            )
            .await
            .expect("test server should stop cleanly");
        });
        (address, shutdown_tx, server)
    }

    async fn wait_for_eof(stream: &mut TcpStream) {
        let mut bytes = Vec::new();
        tokio::time::timeout(Duration::from_secs(1), stream.read_to_end(&mut bytes))
            .await
            .expect("connection should close before the test deadline")
            .expect("connection should be readable");
    }

    #[tokio::test]
    async fn closes_a_connection_when_request_headers_exceed_the_total_deadline() {
        let (address, shutdown, server) = start_server(2, Duration::from_millis(50)).await;
        let mut client = TcpStream::connect(address)
            .await
            .expect("client should connect");
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nX-Pending:")
            .await
            .expect("partial header should be sent");

        wait_for_eof(&mut client).await;
        let _ = shutdown.send(true);
        server.await.expect("test server task should join");
    }

    #[tokio::test]
    async fn rejects_connections_over_the_active_limit_and_releases_slots() {
        let (address, shutdown, server) = start_server(1, Duration::from_secs(2)).await;
        let first = TcpStream::connect(address)
            .await
            .expect("first client should connect");
        tokio::time::sleep(Duration::from_millis(20)).await;

        let mut rejected = TcpStream::connect(address)
            .await
            .expect("second TCP handshake should complete");
        wait_for_eof(&mut rejected).await;
        drop(first);

        let mut next = TcpStream::connect(address)
            .await
            .expect("connection slot should be reusable");
        next.write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .expect("request should be sent");
        let mut response = Vec::new();
        tokio::time::timeout(Duration::from_secs(1), next.read_to_end(&mut response))
            .await
            .expect("response should complete")
            .expect("response should be readable");
        assert!(
            response
                .windows(b"ok 127.0.0.1".len())
                .any(|window| window == b"ok 127.0.0.1"),
            "remote address should reach ConnectInfo, got {response:?}"
        );

        let _ = shutdown.send(true);
        server.await.expect("test server task should join");
    }

    #[tokio::test]
    async fn advertises_bounded_http2_streams_and_extended_connect() {
        let (address, shutdown, server) = start_server(2, Duration::from_secs(2)).await;
        let mut client = TcpStream::connect(address)
            .await
            .expect("client should connect");
        client
            .write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")
            .await
            .expect("HTTP/2 preface should be sent");
        client
            .write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0])
            .await
            .expect("client SETTINGS frame should be sent");

        let mut frame_header = [0; 9];
        tokio::time::timeout(Duration::from_secs(1), client.read_exact(&mut frame_header))
            .await
            .expect("server SETTINGS should arrive")
            .expect("server SETTINGS should be readable");
        assert_eq!(frame_header[3], 4, "first server frame should be SETTINGS");
        let payload_len =
            u32::from_be_bytes([0, frame_header[0], frame_header[1], frame_header[2]]) as usize;
        assert_eq!(payload_len % 6, 0, "SETTINGS payload must contain pairs");
        let mut settings = vec![0; payload_len];
        client
            .read_exact(&mut settings)
            .await
            .expect("SETTINGS payload should be readable");
        let (settings, remainder) = settings.as_chunks::<6>();
        assert!(remainder.is_empty(), "SETTINGS payload must contain pairs");
        let settings = settings
            .iter()
            .map(|setting| {
                let id = u16::from_be_bytes([setting[0], setting[1]]);
                let value = u32::from_be_bytes([setting[2], setting[3], setting[4], setting[5]]);
                (id, value)
            })
            .collect::<Vec<_>>();

        assert!(
            settings.contains(&(3, HTTP2_MAX_CONCURRENT_STREAMS)),
            "server must advertise its stream limit: {settings:?}"
        );
        assert!(
            settings.contains(&(8, 1)),
            "server must preserve Extended CONNECT support: {settings:?}"
        );

        drop(client);
        let _ = shutdown.send(true);
        server.await.expect("test server task should join");
    }
}
