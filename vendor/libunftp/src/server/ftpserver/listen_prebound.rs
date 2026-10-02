//! Contains the shared code for listener modes that prebind control and data connections, including for proxy protocol mode.

use crate::server::failed_logins::FailedLoginsCache;
use crate::server::shutdown;
use crate::server::switchboard::{SocketAddrPair, Switchboard, SwitchboardKey};
use crate::{
    auth::UserDetail,
    server::{
        Reply,
        chancomms::{PortAllocationError, SwitchboardMessage},
        datachan::{self, spawn_processing, spawn_processing_with_socket},
        ftpserver::chosen::OptionsHolder,
        session::SharedSession,
        tls::FtpsConfig,
    },
    storage::StorageBackend,
};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;

pub(super) const MAX_PENDING_PASSIVE_TLS_HANDSHAKES: usize = 64;

// PreboundListener binds to port(s) in advance including passive ports
pub(super) struct PreboundListener<Storage, User>
where
    Storage: StorageBackend<User>,
    User: UserDetail,
{
    pub bind_address: SocketAddr,
    pub logger: slog::Logger,
    #[cfg_attr(not(feature = "proxy_protocol"), allow(dead_code))]
    pub external_control_port: Option<u16>,
    pub options: OptionsHolder<Storage, User>,
    pub switchboard: Switchboard<Storage, User>,
    pub passive_handshake_slots: Arc<tokio::sync::Semaphore>,
    pub shutdown_topic: Arc<shutdown::Notifier>,
    pub failed_logins: Option<Arc<FailedLoginsCache>>,
}

impl<Storage, User> PreboundListener<Storage, User>
where
    Storage: StorageBackend<User> + 'static,
    User: UserDetail + 'static,
{
    pub(crate) async fn handle_switchboard_message(&mut self, msg: SwitchboardMessage<Storage, User>) {
        match msg {
            SwitchboardMessage::AssignDataPortCommand(session_arc, tx) => {
                self.select_and_register_passive_port(session_arc, tx).await;
            }
            // This is sent from the control loop when it exits, so that the port is freed
            SwitchboardMessage::CloseDataPortCommand(session_arc) => {
                let session = session_arc.lock().await;
                if let Some(active_datachan) = &session.switchboard_active_datachan {
                    slog::info!(
                        self.logger,
                        "Unregistering active data channel port because the control channel is closing {:?}",
                        active_datachan
                    );
                    self.switchboard.unregister_by_key(active_datachan);
                }
            }
        }
    }

    // this function finds (by hashing <srcip>.<dstport>) the session
    // that requested this data channel connection in the switchboard
    // hashmap, and then calls the spawn_data_processing function with
    // the tcp_stream
    pub(crate) async fn dispatch_data_connection(&mut self, mut tcp_stream: tokio::net::TcpStream, connection: SocketAddrPair) {
        match self.switchboard.get_session_by_connection_pair(&connection).await {
            Some(session) => {
                let (data_tls, ftps_mode, abort_token, candidate_lock, candidate_slots) = {
                    let session = session.lock().await;
                    (
                        session.data_tls,
                        if session.data_tls { session.ftps_config.clone() } else { FtpsConfig::Off },
                        session.data_abort_tx.as_ref().cloned(),
                        Arc::clone(&session.passive_candidate_lock),
                        Arc::clone(&session.passive_candidate_slots),
                    )
                };

                if !data_tls {
                    spawn_processing(self.logger.clone(), session, tcp_stream).await;
                    self.switchboard.unregister_by_connection_pair(&connection);
                    return;
                }

                let global_slot = match self.passive_handshake_slots.clone().try_acquire_owned() {
                    Ok(slot) => slot,
                    Err(_) => {
                        let _ = tcp_stream.shutdown().await;
                        return;
                    }
                };
                let session_slot = match candidate_slots.try_acquire_owned() {
                    Ok(slot) => slot,
                    Err(_) => {
                        let _ = tcp_stream.shutdown().await;
                        return;
                    }
                };
                let Some(abort_token) = abort_token else {
                    let _ = tcp_stream.shutdown().await;
                    return;
                };

                let mut switchboard = self.switchboard.clone();
                let logger = self.logger.clone();
                tokio::spawn(async move {
                    let _global_slot = global_slot;
                    let _session_slot = session_slot;
                    let _candidate_guard = candidate_lock.lock_owned().await;
                    let expected_key: SwitchboardKey = (&connection).into();
                    let is_current_candidate = {
                        let session_state = session.lock().await;
                        session_state.data_tls && !session_state.data_busy && session_state.switchboard_active_datachan.as_ref() == Some(&expected_key)
                    };
                    if !is_current_candidate {
                        let _ = tcp_stream.shutdown().await;
                        return;
                    }

                    let accept_result = datachan::accept_passive_data_socket(tcp_stream, ftps_mode, abort_token.clone()).await;
                    match accept_result {
                        Ok(socket) => {
                            let is_current_candidate = {
                                let session_state = session.lock().await;
                                session_state.data_tls && !session_state.data_busy && session_state.switchboard_active_datachan.as_ref() == Some(&expected_key)
                            };
                            if !is_current_candidate {
                                drop(socket);
                                return;
                            }
                            spawn_processing_with_socket(logger.clone(), session, socket).await;
                            switchboard.unregister_by_connection_pair(&connection);
                        }
                        Err(_err) if abort_token.is_cancelled() => {}
                        Err(err) => {
                            let error = crate::server::controlchan::sanitize_control_text(&err.to_string());
                            slog::debug!(logger, "Ignoring invalid passive FTPS candidate from {:?}: {}", connection.source, error);
                        }
                    }
                });
            }
            None => {
                slog::warn!(self.logger, "Unexpected connection ({:?})", connection);
                if let Err(e) = tcp_stream.shutdown().await {
                    slog::error!(self.logger, "Error during tcp_stream shutdown: {:?}", e);
                }
            }
        }
    }

    async fn select_and_register_passive_port(&mut self, session_arc: SharedSession<Storage, User>, tx: oneshot::Sender<Result<Reply, PortAllocationError>>) {
        slog::info!(self.logger, "Received internal message to allocate data port");
        // 1. reserve a port
        // 2. put the session_arc and tx in the hashmap with srcip+dstport as key
        // 3. put expiry time in LIFO list
        // 4. send reply to the client: "Entering Passive Mode ({},{},{},{},{},{})"

        let connection = match session_arc.lock().await.control_connection {
            Some(connection) => connection,
            None => {
                slog::error!(self.logger, "Could not allocate data port for session without connection details");
                let _ = tx.send(Err(PortAllocationError));
                return;
            }
        };
        let destination_ip = match connection.destination.ip() {
            IpAddr::V4(ip) => ip,
            IpAddr::V6(_) => {
                slog::warn!(self.logger, "PASV is unavailable on an IPv6 control connection");
                let _ = tx.send(Err(PortAllocationError));
                return;
            }
        };

        let port = self.switchboard.reserve(session_arc.clone()).await;
        let result = match port {
            Ok(port) => Ok(super::controlchan::commands::make_pasv_reply(&self.logger, self.options.passive_host.clone(), &destination_ip, port).await),
            Err(_) => Err(PortAllocationError),
        };

        if tx.send(result).is_err() {
            slog::error!(self.logger, "Could not send port allocation reply to PASV handler");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        auth::AnonymousAuthenticator,
        notification::{DataListener, PresenceListener, nop::NopListener},
        options::{ActivePassiveMode, FtpsRequired, PassiveHost, SiteMd5},
        server::{ftpserver::chosen::OptionsHolder, session::Session, tls::FtpsConfig},
    };
    use rcgen::generate_simple_self_signed;
    use rustls::{
        ClientConfig, RootCertStore, ServerConfig,
        pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName},
    };
    use std::{
        fmt::Debug,
        path::{Path, PathBuf},
        time::{Duration, SystemTime},
    };
    use tokio::{
        io::{AsyncRead, AsyncReadExt},
        net::{TcpListener, TcpStream},
        sync::Mutex,
        time::timeout,
    };
    use tokio_rustls::TlsConnector;
    use unftp_core::{
        auth::{DefaultUser, DefaultUserDetailProvider},
        storage::{Fileinfo, Metadata, Result, StorageBackend},
    };

    #[derive(Debug)]
    struct TestMetadata;

    impl Metadata for TestMetadata {
        fn len(&self) -> u64 {
            0
        }

        fn is_dir(&self) -> bool {
            false
        }

        fn is_file(&self) -> bool {
            false
        }

        fn is_symlink(&self) -> bool {
            false
        }

        fn modified(&self) -> Result<SystemTime> {
            Ok(SystemTime::UNIX_EPOCH)
        }

        fn gid(&self) -> u32 {
            0
        }

        fn uid(&self) -> u32 {
            0
        }
    }

    #[derive(Debug)]
    struct TestStorage;

    #[async_trait::async_trait]
    impl StorageBackend<DefaultUser> for TestStorage {
        type Metadata = TestMetadata;

        async fn metadata<P: AsRef<Path> + Send + Debug>(&self, _: &DefaultUser, _: P) -> Result<Self::Metadata> {
            unreachable!("invalid FTPS candidates must not reach storage")
        }

        async fn list<P: AsRef<Path> + Send + Debug>(&self, _: &DefaultUser, _: P) -> Result<Vec<Fileinfo<PathBuf, Self::Metadata>>> {
            unreachable!("invalid FTPS candidates must not reach storage")
        }

        async fn get<P: AsRef<Path> + Send + Debug>(&self, _: &DefaultUser, _: P, _: u64) -> Result<Box<dyn AsyncRead + Send + Sync + Unpin>> {
            unreachable!("invalid FTPS candidates must not reach storage")
        }

        async fn put<P: AsRef<Path> + Send + Debug, R: AsyncRead + Send + Sync + Unpin + 'static>(&self, _: &DefaultUser, _: R, _: P, _: u64) -> Result<u64> {
            unreachable!("invalid FTPS candidates must not reach storage")
        }

        async fn del<P: AsRef<Path> + Send + Debug>(&self, _: &DefaultUser, _: P) -> Result<()> {
            unreachable!("invalid FTPS candidates must not reach storage")
        }

        async fn mkd<P: AsRef<Path> + Send + Debug>(&self, _: &DefaultUser, _: P) -> Result<()> {
            unreachable!("invalid FTPS candidates must not reach storage")
        }

        async fn rename<P: AsRef<Path> + Send + Debug>(&self, _: &DefaultUser, _: P, _: P) -> Result<()> {
            unreachable!("invalid FTPS candidates must not reach storage")
        }

        async fn rmd<P: AsRef<Path> + Send + Debug>(&self, _: &DefaultUser, _: P) -> Result<()> {
            unreachable!("invalid FTPS candidates must not reach storage")
        }

        async fn cwd<P: AsRef<Path> + Send + Debug>(&self, _: &DefaultUser, _: P) -> Result<()> {
            unreachable!("invalid FTPS candidates must not reach storage")
        }
    }

    fn ftps_config() -> (FtpsConfig, Arc<ClientConfig>) {
        let generated = generate_simple_self_signed(vec!["localhost".to_string()]).expect("test certificate");
        let certificate = CertificateDer::from(generated.cert.der().clone());
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(generated.signing_key.serialize_der()));
        let server_tls = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![certificate.clone()], key)
                .expect("server TLS config"),
        );
        let mut roots = RootCertStore::empty();
        roots.add(certificate).expect("test certificate should be trusted");
        let client_tls = Arc::new(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth());
        let config = FtpsConfig::On {
            tls_config: Arc::clone(&server_tls),
            data_tls_config: None,
            data_resumption: None,
        }
        .for_control_session()
        .expect("per-session FTPS config");
        (config, client_tls)
    }

    #[tokio::test]
    async fn full_tls_handshake_does_not_consume_prebound_passive_mapping() {
        let ftps = ftps_config();
        let logger = slog::Logger::root(slog::Discard {}, slog::o!());
        let passive_listener = TcpListener::bind("127.0.0.1:0").await.expect("passive test listener");
        let passive_address = passive_listener.local_addr().expect("passive listener address");
        let connection = SocketAddrPair {
            source: "127.0.0.1:30000".parse().expect("test source address"),
            destination: passive_address,
        };
        let session = Arc::new(Mutex::new(Session::new(Arc::new(TestStorage), connection.source).ftps(ftps.0.clone())));
        {
            let mut state = session.lock().await;
            state.data_tls = true;
            state.data_abort_tx = Some(tokio_util::sync::CancellationToken::new());
            state.switchboard_active_datachan = Some((&connection).into());
        }
        let mut switchboard = Switchboard::new(logger.clone(), passive_address.port()..=passive_address.port());
        switchboard
            .try_and_claim((&connection).into(), Arc::clone(&session))
            .await
            .expect("passive mapping should be registered");

        let storage: Arc<dyn Fn() -> TestStorage + Send + Sync> = Arc::new(|| TestStorage);
        let data_listener: Arc<dyn DataListener> = Arc::new(NopListener {});
        let presence_listener: Arc<dyn PresenceListener> = Arc::new(NopListener {});
        let options = OptionsHolder {
            storage,
            greeting: "test",
            authenticator: Arc::new(AnonymousAuthenticator {}),
            user_detail_provider: Arc::new(DefaultUserDetailProvider {}),
            passive_ports: passive_address.port()..=passive_address.port(),
            passive_host: PassiveHost::default(),
            ftps_config: ftps.0,
            collect_metrics: false,
            idle_session_timeout: Duration::from_secs(30),
            logger: logger.clone(),
            ftps_required_control_chan: FtpsRequired::All,
            ftps_required_data_chan: FtpsRequired::All,
            site_md5: SiteMd5::None,
            data_listener,
            presence_listener,
            active_passive_mode: ActivePassiveMode::PassiveOnly,
            binder: Arc::new(std::sync::Mutex::new(None)),
        };
        let mut listener = PreboundListener {
            bind_address: "127.0.0.1:0".parse().expect("test bind address"),
            logger,
            external_control_port: None,
            options,
            switchboard: switchboard.clone(),
            passive_handshake_slots: Arc::new(tokio::sync::Semaphore::new(2)),
            shutdown_topic: Arc::new(crate::server::shutdown::Notifier::new()),
            failed_logins: None,
        };

        for _ in 0..2 {
            let client_stream = TcpStream::connect(passive_address).await.expect("candidate should connect");
            let (server_stream, peer) = passive_listener.accept().await.expect("candidate should be accepted");
            let candidate = SocketAddrPair {
                source: peer,
                destination: passive_address,
            };
            listener.dispatch_data_connection(server_stream, candidate.clone()).await;

            let connector = TlsConnector::from(Arc::clone(&ftps.1));
            let mut tls_stream = connector
                .connect(ServerName::try_from("localhost").expect("server name"), client_stream)
                .await
                .expect("a valid full TLS handshake should complete before resumption rejection");
            assert_eq!(tls_stream.get_ref().1.handshake_kind(), Some(rustls::HandshakeKind::Full));
            let mut byte = [0_u8; 1];
            let read_result = timeout(Duration::from_secs(2), tls_stream.read(&mut byte))
                .await
                .expect("rejected candidate should be closed promptly");
            assert!(!matches!(read_result, Ok(count) if count > 0), "rejected candidate must not receive data");

            timeout(Duration::from_secs(2), async {
                while session.lock().await.passive_candidate_slots.available_permits() != 2 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("candidate semaphore permit should be released");
            assert!(
                switchboard.get_session_by_connection_pair(&candidate).await.is_some(),
                "a full handshake that did not resume the control session must leave the passive mapping available"
            );
            assert!(!session.lock().await.data_busy, "rejected candidate must not start a data worker");
        }
    }
}
