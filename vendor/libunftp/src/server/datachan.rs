//! Contains code pertaining to the FTP *data* channel

use super::{
    chancomms::{ControlChanMsg, DataChanMsg},
    tls::FtpsConfig,
};
use crate::server::session::SharedSession;
use crate::{
    auth::UserDetail,
    storage::{Error, ErrorKind, Metadata, StorageBackend},
};

use crate::server::chancomms::DataChanCmd;
use crate::server::controlchan::sanitize_control_text;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;
use tokio::sync::{
    mpsc::{Receiver, Sender},
    oneshot,
};
use tokio_rustls::TlsAcceptor;

use crate::metrics;
use rustls::HandshakeKind;
use tokio_util::sync::{CancellationToken, WaitForCancellationFutureOwned};

#[derive(Debug)]
struct DataCommandExecutor<Storage, User>
where
    Storage: StorageBackend<User>,
    Storage::Metadata: Metadata,
    User: UserDetail,
{
    pub user: Arc<Option<User>>,
    pub socket: DataSocket,
    pub control_msg_tx: Sender<ControlChanMsg>,
    pub storage: Arc<Storage>,
    pub cwd: PathBuf,
    pub ftps_mode: FtpsConfig,
    pub logger: slog::Logger,
    pub data_cmd_rx: Option<Receiver<DataChanCmd>>,
    pub abort_token: CancellationToken,
}

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

const DATA_CHANNEL_COMMAND_TIMEOUT: Duration = Duration::from_secs(60);
const DATA_CHANNEL_TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
const DATA_CHANNEL_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const PASSIVE_CANDIDATE_TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(crate) enum DataSocket {
    Tcp(TcpStream),
    Tls(tokio_rustls::server::TlsStream<TcpStream>),
}

impl DataSocket {
    fn peer_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        match self {
            Self::Tcp(socket) => socket.peer_addr(),
            Self::Tls(stream) => stream.get_ref().0.peer_addr(),
        }
    }

    async fn shutdown(&mut self) -> std::io::Result<()> {
        match self {
            Self::Tcp(socket) => socket.shutdown().await,
            Self::Tls(stream) => stream.shutdown().await,
        }
    }
}

pub(crate) async fn accept_passive_data_socket(socket: TcpStream, ftps_mode: FtpsConfig, abort_token: CancellationToken) -> std::io::Result<DataSocket> {
    match ftps_mode {
        FtpsConfig::Off => Ok(DataSocket::Tcp(socket)),
        FtpsConfig::Building { .. } => Err(std::io::Error::other("Illegal FTPS data-channel state")),
        FtpsConfig::On {
            data_tls_config: Some(tls_config),
            data_resumption: Some(data_resumption),
            ..
        } => {
            let acceptor: TlsAcceptor = tls_config.into();
            let reset_resumption = Arc::clone(&data_resumption);
            let tls_stream = tokio::select! {
                accepted = tokio::time::timeout(
                    PASSIVE_CANDIDATE_TLS_HANDSHAKE_TIMEOUT,
                    acceptor.accept_with(socket, move |_| {
                        reset_resumption.store(false, Ordering::SeqCst);
                    }),
                ) => {
                    accepted
                        .map_err(|_| std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "FTPS passive candidate handshake timed out",
                        ))?
                        .map_err(std::io::Error::other)?
                }
                _ = abort_token.cancelled() => return Err(aborted_io_error()),
            };
            require_control_session_resumption(&tls_stream, &data_resumption)?;
            Ok(DataSocket::Tls(tls_stream))
        }
        FtpsConfig::On { .. } => Err(std::io::Error::other("Missing per-session FTPS data configuration")),
    }
}

fn require_control_session_resumption(stream: &tokio_rustls::server::TlsStream<TcpStream>, data_resumption: &AtomicBool) -> std::io::Result<()> {
    match (stream.get_ref().1.handshake_kind(), data_resumption.load(Ordering::SeqCst)) {
        (Some(HandshakeKind::Resumed), true) => Ok(()),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "FTPS data channel did not resume the control-channel TLS session",
        )),
    }
}

struct IdleTimeoutReader<R> {
    inner: R,
    deadline: Pin<Box<tokio::time::Sleep>>,
}

impl<R> IdleTimeoutReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            deadline: Box::pin(tokio::time::sleep(DATA_CHANNEL_IDLE_TIMEOUT)),
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for IdleTimeoutReader<R> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        let this = self.as_mut().get_mut();
        if this.deadline.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "FTP data-channel read timed out while idle",
            )));
        }
        match Pin::new(&mut this.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                if !buf.filled().is_empty() {
                    this.deadline.as_mut().reset(tokio::time::Instant::now() + DATA_CHANNEL_IDLE_TIMEOUT);
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => Poll::Pending,
        }
    }
}

struct IdleTimeoutWriter<W> {
    inner: W,
    deadline: Pin<Box<tokio::time::Sleep>>,
}

impl<W> IdleTimeoutWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            deadline: Box::pin(tokio::time::sleep(DATA_CHANNEL_IDLE_TIMEOUT)),
        }
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for IdleTimeoutWriter<W> {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        let this = self.as_mut().get_mut();
        if this.deadline.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "FTP data-channel write timed out while idle",
            )));
        }
        match Pin::new(&mut this.inner).poll_write(cx, buf) {
            Poll::Ready(Ok(bytes_written)) => {
                if bytes_written > 0 {
                    this.deadline.as_mut().reset(tokio::time::Instant::now() + DATA_CHANNEL_IDLE_TIMEOUT);
                }
                Poll::Ready(Ok(bytes_written))
            }
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.as_mut().get_mut();
        if this.deadline.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "FTP data-channel flush timed out while idle",
            )));
        }
        match Pin::new(&mut this.inner).poll_flush(cx) {
            Poll::Ready(result) => Poll::Ready(result),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.as_mut().get_mut();
        if this.deadline.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "FTP data-channel shutdown timed out while idle",
            )));
        }
        match Pin::new(&mut this.inner).poll_shutdown(cx) {
            Poll::Ready(result) => Poll::Ready(result),
            Poll::Pending => Poll::Pending,
        }
    }
}

struct MeasuringWriter<W> {
    writer: W,
    command: &'static str,
}

struct MeasuringReader<R> {
    reader: R,
    command: &'static str,
}

impl<W: AsyncWrite + Unpin> AsyncWrite for MeasuringWriter<W> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> std::task::Poll<Result<usize, std::io::Error>> {
        let this = self.get_mut();

        let result = Pin::new(&mut this.writer).poll_write(cx, buf);
        if let Poll::Ready(Ok(bytes_written)) = &result {
            metrics::inc_sent_bytes(*bytes_written, this.command);
        }

        result
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.writer).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.writer).poll_shutdown(cx)
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for MeasuringReader<R> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.reader).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &result {
            let bytes_read = buf.filled().len();
            metrics::inc_received_bytes(bytes_read, this.command);
        }
        result
    }
}

impl<W> MeasuringWriter<W> {
    fn new(writer: W, command: &'static str) -> MeasuringWriter<W> {
        Self { writer, command }
    }
}

impl<R> MeasuringReader<R> {
    fn new(reader: R, command: &'static str) -> MeasuringReader<R> {
        Self { reader, command }
    }
}

struct AbortableReader<R> {
    inner: R,
    abort_wait: Pin<Box<WaitForCancellationFutureOwned>>,
}

impl<R> AbortableReader<R> {
    fn new(inner: R, abort_token: CancellationToken) -> Self {
        Self {
            inner,
            abort_wait: Box::pin(abort_token.cancelled_owned()),
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for AbortableReader<R> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if this.abort_wait.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(aborted_io_error()));
        }
        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

struct AbortableWriter<W> {
    inner: W,
    abort_wait: Pin<Box<WaitForCancellationFutureOwned>>,
}

impl<W> AbortableWriter<W> {
    fn new(inner: W, abort_token: CancellationToken) -> Self {
        Self {
            inner,
            abort_wait: Box::pin(abort_token.cancelled_owned()),
        }
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for AbortableWriter<W> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        if this.abort_wait.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(aborted_io_error()));
        }
        Pin::new(&mut this.inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if this.abort_wait.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(aborted_io_error()));
        }
        Pin::new(&mut this.inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if this.abort_wait.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(aborted_io_error()));
        }
        Pin::new(&mut this.inner).poll_shutdown(cx)
    }
}

fn aborted_io_error() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "FTP data channel aborted")
}

impl<Storage, User> DataCommandExecutor<Storage, User>
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail + 'static,
{
    async fn execute(mut self, session_arc: SharedSession<Storage, User>) {
        let mut data_cmd_rx = self.data_cmd_rx.take().unwrap();
        let mut timeout_delay = Box::pin(tokio::time::sleep(DATA_CHANNEL_COMMAND_TIMEOUT));
        let command = tokio::select! {
            Some(command) = data_cmd_rx.recv() => Some(command),
            _ = self.abort_token.cancelled() => None,
            _ = &mut timeout_delay => {
                slog::warn!(self.logger, "Data channel connection timed out");
                None
            }
        };
        if let Some(command) = command {
            // Copy the restart offset, then release the session lock before storage or network I/O.
            let start_pos = session_arc.lock().await.start_pos;
            self.handle_incoming(DataChanMsg::ExternalCommand(command), start_pos).await;
        } else if self.abort_token.is_cancelled() {
            slog::info!(self.logger, "Data channel abort received");
        }
        let mut session = session_arc.lock().await;
        session.data_busy = false;
    }

    #[tracing_attributes::instrument(skip_all)]
    async fn handle_incoming(self, incoming: DataChanMsg, start_pos: u64) {
        match incoming {
            DataChanMsg::Abort => {
                slog::info!(self.logger, "Data channel abort received");
            }
            DataChanMsg::ExternalCommand(command) => {
                let p = command.path().unwrap_or_default();
                let command_name = match &command {
                    DataChanCmd::Retr { .. } => "RETR",
                    DataChanCmd::Stor { .. } => "STOR",
                    DataChanCmd::Appe { .. } => "APPE",
                    DataChanCmd::List { .. } => "LIST",
                    DataChanCmd::Nlst { .. } => "NLST",
                    DataChanCmd::Mlsd { .. } => "MLSD",
                };
                slog::debug!(self.logger, "Data channel command received: {}", command_name; "path" => sanitize_control_text(&p));
                self.execute_command(command, start_pos).await;
            }
        }
    }

    #[tracing_attributes::instrument(skip_all)]
    async fn execute_command(self, cmd: DataChanCmd, start_pos: u64) {
        match cmd {
            DataChanCmd::Retr { path } => {
                self.exec_retr(path, start_pos).await;
            }
            DataChanCmd::Stor { path } => {
                self.exec_stor(path, start_pos).await;
            }
            DataChanCmd::Appe { path } => {
                self.exec_appe(path).await;
            }
            DataChanCmd::List { path, .. } => {
                self.exec_list_variant(path, ListCommand::List).await;
            }
            DataChanCmd::Nlst { path } => {
                self.exec_list_variant(path, ListCommand::Nlst).await;
            }
            DataChanCmd::Mlsd { path } => {
                self.exec_mlsd(path).await;
            }
        }
    }

    #[tracing_attributes::instrument(skip_all)]
    async fn exec_retr(self, path: String, start_pos: u64) {
        let path_copy = path.clone();
        let log_path = sanitize_control_text(&path_copy);
        let path = self.cwd.join(path);
        let tx: Sender<ControlChanMsg> = self.control_msg_tx.clone();
        let logger = self.logger.clone();
        let mut output = match Self::writer(self.socket, self.ftps_mode, "retr", self.abort_token.clone()).await {
            Ok(output) => output,
            Err(err) => {
                Self::report_tls_handshake_failure(logger, tx, "RETR", err).await;
                return;
            }
        };

        let start_time = Instant::now();
        let result = self.storage.get_into((*self.user).as_ref().unwrap(), path, start_pos, &mut output).await;

        if let Err(err) = output.shutdown().await {
            match err.kind() {
                std::io::ErrorKind::BrokenPipe => {
                    slog::debug!(self.logger, "Output stream was already closed by peer after RETR: {:?}", err);
                }
                std::io::ErrorKind::NotConnected => {
                    slog::debug!(self.logger, "Output stream was already closed after RETR: {:?}", err);
                }
                _ => slog::warn!(self.logger, "Could not shutdown output stream after RETR: {:?}", err),
            }
        }

        let duration = start_time.elapsed();
        match result {
            Ok(bytes_copied) => {
                slog::info!(
                    self.logger,
                    "Successful RETR {:?}; Duration {}; Bytes copied {}; Transfer speed {}; start_pos={}",
                    &log_path,
                    HumanDuration(duration),
                    HumanBytes(bytes_copied),
                    TransferSpeed(bytes_copied as f64 / duration.as_secs_f64()),
                    start_pos,
                );

                // only register transfer of a single file transfer
                if start_pos == 0 {
                    metrics::inc_transferred("retr", "success");
                }

                if let Err(_err) = tx
                    .send(ControlChanMsg::SentData {
                        bytes: bytes_copied,
                        path: path_copy,
                    })
                    .await
                {
                    slog::error!(self.logger, "Could not notify control channel of successful RETR");
                }
            }
            Err(err) => {
                let io_error_kind = err.get_io_error().map(|e| e.kind());

                if io_error_kind == Some(std::io::ErrorKind::BrokenPipe) {
                    if start_pos == 0 {
                        slog::warn!(
                            self.logger,
                            "Client halted RETR transfer (BrokenPipe). Certain FTP clients may do this to download file sections separately, in which case RESTarts may occur and will be logged at DEBUG level. Refer to your FTP client's documentation if this causes issues. Path {:?}; Duration {} (number of bytes copied unknown).",
                            &log_path,
                            HumanDuration(duration)
                        );
                    } else {
                        slog::debug!(
                            self.logger,
                            "RETR transfer stopped by client (BrokenPipe). Remember, this could be standard for some FTP clients. Path {:?}; Duration {} (number of bytes copied unknown); start_pos {}",
                            &log_path,
                            HumanDuration(duration),
                            start_pos
                        );
                    }
                } else {
                    let log_error = sanitize_control_text(&format!("{err:?}"));
                    slog::warn!(
                        self.logger,
                        "Error during RETR {:?} transfer after {}: {}; start_pos={}",
                        &log_path,
                        HumanDuration(duration),
                        log_error,
                        start_pos
                    );
                }

                // only register transfer errors for a single file transfer once
                if start_pos == 0 {
                    categorize_and_register_error(&self.logger, &err, "retr");
                }

                if let Err(_err) = tx.send(ControlChanMsg::StorageError(err)).await {
                    slog::warn!(self.logger, "Could not notify control channel of error with RETR");
                }
            }
        }
    }

    #[tracing_attributes::instrument(skip_all)]
    async fn exec_stor(self, path: String, start_pos: u64) {
        let path_copy = path.clone();
        let log_path = sanitize_control_text(&path_copy);
        let path = self.cwd.join(path);
        let tx = self.control_msg_tx.clone();
        let logger = self.logger.clone();
        let input = match Self::reader(self.socket, self.ftps_mode, "stor", self.abort_token.clone()).await {
            Ok(input) => input,
            Err(err) => {
                Self::report_tls_handshake_failure(logger, tx, "STOR", err).await;
                return;
            }
        };

        let start_time = Instant::now();
        let put_result = self.storage.put((*self.user).as_ref().unwrap(), input, path, start_pos).await;
        let duration = start_time.elapsed();

        match put_result {
            Ok(bytes) => {
                slog::info!(
                    self.logger,
                    "Successful STOR {:?}; Duration {}; Bytes copied {}; Transfer speed {}; start_pos={}",
                    &log_path,
                    HumanDuration(duration),
                    HumanBytes(bytes),
                    TransferSpeed(bytes as f64 / duration.as_secs_f64()),
                    start_pos,
                );

                // only register transfer of a single file transfer
                if start_pos == 0 {
                    metrics::inc_transferred("stor", "success");
                }

                if let Err(_err) = tx.send(ControlChanMsg::WrittenData { bytes, path: path_copy }).await {
                    slog::error!(self.logger, "Could not notify control channel of successful STOR");
                }
            }
            Err(err) => {
                let log_error = sanitize_control_text(&format!("{err:?}"));
                slog::warn!(self.logger, "Error during STOR transfer after {}: {}", HumanDuration(duration), log_error);

                // only register transfer errors for a single file transfer once
                if start_pos == 0 {
                    categorize_and_register_error(&self.logger, &err, "stor");
                }

                if let Err(_err) = tx.send(ControlChanMsg::StorageError(err)).await {
                    slog::error!(self.logger, "Could not notify control channel of error with STOR");
                }
            }
        }
    }

    #[tracing_attributes::instrument(skip_all)]
    async fn exec_appe(self, path: String) {
        let path_copy = path.clone();
        let log_path = sanitize_control_text(&path_copy);
        let full_path = self.cwd.join(&path);
        let tx = self.control_msg_tx.clone();

        // Get current file size, or 0 if file doesn't exist
        let start_pos = match self.storage.metadata((*self.user).as_ref().unwrap(), &full_path).await {
            Ok(meta) => meta.len(),
            Err(err) if err.kind() == ErrorKind::PermanentFileNotAvailable => 0,
            Err(err) => {
                slog::warn!(self.logger, "APPE refused because the existing file size could not be determined");
                categorize_and_register_error(&self.logger, &err, "appe");
                if let Err(_send_err) = tx.send(ControlChanMsg::StorageError(err)).await {
                    slog::warn!(self.logger, "Could not notify control channel of APPE metadata error");
                }
                return;
            }
        };

        let logger = self.logger.clone();
        let input = match Self::reader(self.socket, self.ftps_mode, "appe", self.abort_token.clone()).await {
            Ok(input) => input,
            Err(err) => {
                Self::report_tls_handshake_failure(logger, tx, "APPE", err).await;
                return;
            }
        };

        let start_time = Instant::now();
        let put_result = self.storage.put((*self.user).as_ref().unwrap(), input, full_path, start_pos).await;
        let duration = start_time.elapsed();

        match put_result {
            Ok(bytes) => {
                slog::info!(
                    self.logger,
                    "Successful APPE {:?}; Duration {}; Bytes copied {}; Transfer speed {}; start_pos={}",
                    &log_path,
                    HumanDuration(duration),
                    HumanBytes(bytes),
                    TransferSpeed(bytes as f64 / duration.as_secs_f64()),
                    start_pos,
                );

                metrics::inc_transferred("appe", "success");

                if let Err(_err) = tx.send(ControlChanMsg::WrittenData { bytes, path: path_copy }).await {
                    slog::error!(self.logger, "Could not notify control channel of successful APPE");
                }
            }
            Err(err) => {
                let log_error = sanitize_control_text(&format!("{err:?}"));
                slog::warn!(self.logger, "Error during APPE transfer after {}: {}", HumanDuration(duration), log_error);

                categorize_and_register_error(&self.logger, &err, "appe");

                if let Err(_err) = tx.send(ControlChanMsg::StorageError(err)).await {
                    slog::error!(self.logger, "Could not notify control channel of error with APPE");
                }
            }
        }
    }

    #[tracing_attributes::instrument(skip_all)]
    async fn exec_list_variant(self, path: Option<String>, command: ListCommand) {
        let path = self.resolve_path(path);
        let log_path = crate::server::controlchan::sanitize_control_path(&path);
        let tx = self.control_msg_tx.clone();
        let logger = self.logger.clone();
        let mut output = match Self::writer(self.socket, self.ftps_mode.clone(), command.as_lower_str(), self.abort_token.clone()).await {
            Ok(output) => output,
            Err(err) => {
                Self::report_tls_handshake_failure(logger, tx, command.as_str(), err).await;
                return;
            }
        };

        let start_time = Instant::now();

        let list_result = match command {
            ListCommand::List => self.storage.list_fmt((*self.user).as_ref().unwrap(), path.clone()).await,
            ListCommand::Nlst => self
                .storage
                .nlst((*self.user).as_ref().unwrap(), path.clone())
                .await
                .map_err(|e| Error::new(ErrorKind::PermanentDirectoryNotAvailable, e)),
        };

        match list_result {
            Ok(cursor) => {
                slog::debug!(self.logger, "Copying future for {}", command.as_str());
                let mut input = cursor;
                let result = tokio::io::copy(&mut input, &mut output).await;

                if let Err(err) = output.shutdown().await {
                    match err.kind() {
                        std::io::ErrorKind::BrokenPipe => {
                            slog::debug!(self.logger, "Output stream was already closed by peer after {}: {:?}", command.as_str(), err);
                        }
                        std::io::ErrorKind::NotConnected => {
                            slog::debug!(self.logger, "Output stream was already closed after {}: {:?}", command.as_str(), err);
                        }
                        _ => slog::warn!(self.logger, "Could not shutdown output stream after {}: {:?}", command.as_str(), err),
                    }
                }
                let duration = start_time.elapsed();

                match result {
                    Ok(bytes) => {
                        slog::info!(
                            self.logger,
                            "Successful LIST {:?}; Duration {}; Bytes copied {}; Transfer speed {}",
                            log_path,
                            HumanDuration(duration),
                            HumanBytes(bytes),
                            TransferSpeed(bytes as f64 / duration.as_secs_f64()),
                        );
                        metrics::inc_transferred(command.as_lower_str(), "success");
                        if let Err(_err) = tx.send(ControlChanMsg::DirectorySuccessfullyListed).await {
                            slog::error!(self.logger, "Could not notify control channel of successful {}", command.as_str());
                        }
                    }
                    Err(e) => {
                        let duration = start_time.elapsed();
                        let log_error = sanitize_control_text(&format!("{e:?}"));
                        slog::warn!(
                            self.logger,
                            "Failed to send directory list for path {:?} ({} command) after {}: {}",
                            log_path,
                            command.as_str(),
                            HumanDuration(duration),
                            log_error,
                        );

                        let err = Error::from(e);
                        categorize_and_register_error(&self.logger, &err, command.as_lower_str());
                    }
                }
            }
            Err(err) => {
                let duration = start_time.elapsed();
                let log_error = sanitize_control_text(&format!("{err:?}"));

                slog::warn!(
                    self.logger,
                    "Failed to retrieve directory list for path {:?} ({} command) from storage backend after {}: {:?}",
                    log_path,
                    command.as_str(),
                    HumanDuration(duration),
                    log_error,
                );

                categorize_and_register_error(&self.logger, &err, command.as_lower_str());

                if let Err(_err) = tx.send(ControlChanMsg::StorageError(err)).await {
                    slog::error!(self.logger, "Could not notify control channel of error with {}", command.as_str());
                }
            }
        }
    }

    #[tracing_attributes::instrument(skip_all)]
    async fn exec_mlsd(self, path: Option<String>) {
        let path = self.resolve_path(path);
        let log_path = crate::server::controlchan::sanitize_control_path(&path);
        let tx = self.control_msg_tx.clone();
        let logger = self.logger.clone();
        let mut output = match Self::writer(self.socket, self.ftps_mode.clone(), "mlsd", self.abort_token.clone()).await {
            Ok(output) => output,
            Err(err) => {
                Self::report_tls_handshake_failure(logger, tx, "MLSD", err).await;
                return;
            }
        };

        let start_time = Instant::now();

        let list_result = self.storage.list((*self.user).as_ref().unwrap(), path.clone()).await;

        match list_result {
            Ok(files) => {
                let mut buffer = String::new();
                for file_info in files {
                    let filename = file_info
                        .path
                        .as_path()
                        .components()
                        .next_back()
                        .map(|component| component.as_os_str().to_string_lossy())
                        .unwrap_or("unknown".into());

                    let facts_str = crate::server::controlchan::commands::mlst::format_facts(&file_info.metadata);
                    let line = format!("{} {}", facts_str, filename);
                    buffer.push_str(&line);
                    buffer.push_str("\r\n");
                }

                // Send the formatted data
                let mut input = std::io::Cursor::new(buffer.into_bytes());
                let result = tokio::io::copy(&mut input, &mut output).await;

                if let Err(err) = output.shutdown().await {
                    match err.kind() {
                        std::io::ErrorKind::BrokenPipe => {
                            slog::debug!(self.logger, "Output stream was already closed by peer after MLSD: {:?}", err);
                        }
                        std::io::ErrorKind::NotConnected => {
                            slog::debug!(self.logger, "Output stream was already closed after MLSD: {:?}", err);
                        }
                        _ => slog::warn!(self.logger, "Could not shutdown output stream after MLSD: {:?}", err),
                    }
                }

                let duration = start_time.elapsed();

                match result {
                    Ok(bytes) => {
                        slog::info!(
                            self.logger,
                            "Successful MLSD {:?}; Duration {}; Bytes copied {}; Transfer speed {}",
                            log_path,
                            HumanDuration(duration),
                            HumanBytes(bytes),
                            TransferSpeed(bytes as f64 / duration.as_secs_f64())
                        );

                        metrics::inc_transferred("mlsd", "success");

                        if let Err(_err) = tx.send(ControlChanMsg::DirectorySuccessfullyListed).await {
                            slog::error!(self.logger, "Could not notify control channel of successful MLSD");
                        }
                    }
                    Err(err) => {
                        let log_error = sanitize_control_text(&format!("{err:?}"));
                        slog::warn!(
                            self.logger,
                            "Failed to copy MLSD data to client after {}. Error: {}",
                            HumanDuration(duration),
                            log_error
                        );
                        if let Err(_err) = tx.send(ControlChanMsg::WriteFailed).await {
                            slog::error!(self.logger, "Could not notify control channel of failed MLSD");
                        }
                    }
                }
            }
            Err(err) => {
                let duration = start_time.elapsed();
                let log_error = sanitize_control_text(&format!("{err:?}"));
                slog::warn!(
                    self.logger,
                    "Failed to retrieve directory list for path {:?} (MLSD command) from storage backend after {}: {:?}",
                    log_path,
                    HumanDuration(duration),
                    log_error,
                );

                categorize_and_register_error(&self.logger, &err, "mlsd");

                if let Err(_err) = tx.send(ControlChanMsg::StorageError(err)).await {
                    slog::error!(self.logger, "Could not notify control channel of error with MLSD");
                }
            }
        }
    }

    #[tracing_attributes::instrument(skip_all)]
    async fn writer(
        socket: DataSocket,
        ftps_mode: FtpsConfig,
        command: &'static str,
        abort_token: CancellationToken,
    ) -> std::io::Result<Box<dyn AsyncWrite + Send + Unpin + Sync>> {
        let writer: Box<dyn AsyncWrite + Send + Unpin + Sync> = match ftps_mode {
            FtpsConfig::Off => match socket {
                DataSocket::Tcp(socket) => Box::new(MeasuringWriter::new(socket, command)),
                DataSocket::Tls(_) => return Err(std::io::Error::other("Unexpected TLS data channel")),
            },
            FtpsConfig::Building { .. } => {
                return Err(std::io::Error::other("Illegal FTPS data-channel state"));
            }
            FtpsConfig::On {
                data_tls_config: Some(tls_config),
                data_resumption: Some(data_resumption),
                ..
            } => {
                let tls_stream = match socket {
                    DataSocket::Tls(stream) => stream,
                    DataSocket::Tcp(socket) => {
                        let acceptor: TlsAcceptor = tls_config.into();
                        let reset_resumption = Arc::clone(&data_resumption);
                        tokio::select! {
                            accepted = tokio::time::timeout(DATA_CHANNEL_TLS_HANDSHAKE_TIMEOUT, acceptor.accept_with(socket, move |_| {
                                reset_resumption.store(false, Ordering::SeqCst);
                            })) => {
                                accepted
                                    .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "FTPS data-channel handshake timed out"))?
                                    .map_err(std::io::Error::other)?
                            }
                            _ = abort_token.cancelled() => return Err(aborted_io_error()),
                        }
                    }
                };
                require_control_session_resumption(&tls_stream, &data_resumption)?;
                Box::new(MeasuringWriter::new(tls_stream, command))
            }
            FtpsConfig::On { .. } => return Err(std::io::Error::other("Missing per-session FTPS data configuration")),
        };
        Ok(Box::new(IdleTimeoutWriter::new(AbortableWriter::new(writer, abort_token))))
    }

    #[tracing_attributes::instrument(skip_all)]
    async fn reader(
        socket: DataSocket,
        ftps_mode: FtpsConfig,
        command: &'static str,
        abort_token: CancellationToken,
    ) -> std::io::Result<Box<dyn AsyncRead + Send + Unpin + Sync>> {
        let reader: Box<dyn AsyncRead + Send + Unpin + Sync> = match ftps_mode {
            FtpsConfig::Off => match socket {
                DataSocket::Tcp(socket) => Box::new(MeasuringReader::new(socket, command)),
                DataSocket::Tls(_) => return Err(std::io::Error::other("Unexpected TLS data channel")),
            },
            FtpsConfig::Building { .. } => {
                return Err(std::io::Error::other("Illegal FTPS data-channel state"));
            }
            FtpsConfig::On {
                data_tls_config: Some(tls_config),
                data_resumption: Some(data_resumption),
                ..
            } => {
                let tls_stream = match socket {
                    DataSocket::Tls(stream) => stream,
                    DataSocket::Tcp(socket) => {
                        let acceptor: TlsAcceptor = tls_config.into();
                        let reset_resumption = Arc::clone(&data_resumption);
                        tokio::select! {
                            accepted = tokio::time::timeout(DATA_CHANNEL_TLS_HANDSHAKE_TIMEOUT, acceptor.accept_with(socket, move |_| {
                                reset_resumption.store(false, Ordering::SeqCst);
                            })) => {
                                accepted
                                    .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "FTPS data-channel handshake timed out"))?
                                    .map_err(std::io::Error::other)?
                            }
                            _ = abort_token.cancelled() => return Err(aborted_io_error()),
                        }
                    }
                };
                require_control_session_resumption(&tls_stream, &data_resumption)?;
                Box::new(MeasuringReader::new(tls_stream, command))
            }
            FtpsConfig::On { .. } => return Err(std::io::Error::other("Missing per-session FTPS data configuration")),
        };
        Ok(Box::new(IdleTimeoutReader::new(AbortableReader::new(reader, abort_token))))
    }

    async fn report_tls_handshake_failure(logger: slog::Logger, tx: Sender<ControlChanMsg>, command: &'static str, err: std::io::Error) {
        slog::warn!(logger, "{} data-channel TLS handshake failed: {}", command, err);
        if let Err(send_error) = tx.send(ControlChanMsg::ConnectionReset).await {
            slog::warn!(
                logger,
                "Could not notify control channel about the {} data-channel failure: {}",
                command,
                send_error
            );
        }
    }

    fn resolve_path(&self, path: Option<String>) -> PathBuf {
        match path {
            Some(path) => {
                if path == "." {
                    self.cwd.clone()
                } else {
                    self.cwd.join(path)
                }
            }
            None => self.cwd.clone(),
        }
    }
}

/// Starts processing for the data connection. This will spawn a new async task that will wait for
/// a command from the control channel after which it will start to process the specified socket
/// that is connected to the client.
///
/// logger: logger set up with needed context for use by the data channel.
/// session_arc: the user session that is also shared with the control channel.
/// socket: the data socket we'll be working with.
#[tracing_attributes::instrument(skip_all)]
pub async fn spawn_processing<Storage, User>(logger: slog::Logger, session_arc: SharedSession<Storage, User>, socket: TcpStream)
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail + 'static,
{
    spawn_processing_with_socket(logger, session_arc, DataSocket::Tcp(socket)).await;
}

pub(crate) async fn spawn_processing_with_socket<Storage, User>(logger: slog::Logger, session_arc: SharedSession<Storage, User>, mut socket: DataSocket)
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail + 'static,
{
    // We introduce a block scope here to keep the lock on the session minimal. We basically copy the needed info
    // out and then unlock.

    let command_executor = {
        let mut session = session_arc.lock().await;

        match socket.peer_addr() {
            Ok(datachan_addr) => {
                let controlchan_ip = session.source.ip();
                if controlchan_ip != datachan_addr.ip() {
                    if let Err(err) = socket.shutdown().await {
                        slog::error!(
                            logger,
                            "Couldn't close datachannel for IP ({}) that does not match the IP({}) of the control channel: {:?}",
                            datachan_addr.ip(),
                            controlchan_ip,
                            err
                        )
                    } else {
                        slog::warn!(
                            logger,
                            "Closing datachannel for IP ({}) that does not match the IP({}) of the control channel.",
                            datachan_addr.ip(),
                            controlchan_ip
                        )
                    }
                    return;
                }
            }
            Err(err) => {
                slog::error!(logger, "Couldn't determine data channel address: {:?}", err);
                return;
            }
        }

        if session.data_busy {
            slog::warn!(logger, "Closing additional data connection while this session is busy");
            return;
        }

        let username = session.username.as_ref().cloned().unwrap_or_else(|| String::from("unknown"));
        let logger = logger.new(slog::o!("username" => username));
        let control_msg_tx: Sender<ControlChanMsg> = match session.control_msg_tx {
            Some(ref tx) => tx.clone(),
            None => {
                slog::error!(logger, "Control loop message sender expected to be set up. Aborting data loop.");
                return;
            }
        };
        let data_cmd_rx = match session.data_cmd_rx.take() {
            Some(rx) => rx,
            None => {
                slog::error!(logger, "Data loop command receiver expected to be set up. Aborting data loop.");
                return;
            }
        };
        let abort_token = match session.data_abort_tx.as_ref().cloned() {
            Some(token) => token,
            None => {
                slog::error!(logger, "Data loop abort token expected to be set up. Aborting data loop.");
                return;
            }
        };
        let ftps_mode = if session.data_tls { session.ftps_config.clone() } else { FtpsConfig::Off };
        let command_executor = DataCommandExecutor {
            user: session.user.clone(),
            socket,
            control_msg_tx,
            storage: Arc::clone(&session.storage),
            cwd: session.cwd.clone(),
            ftps_mode,
            logger,
            abort_token,
            data_cmd_rx: Some(data_cmd_rx),
        };

        // The control channel need to know if the data channel is busy so that it doesn't time out
        // while the session is still in progress.
        session.data_busy = true;

        command_executor
    };

    let (start_tx, start_rx) = oneshot::channel();
    let task_session = session_arc.clone();
    let task = tokio::spawn(async move {
        if start_rx.await.is_ok() {
            command_executor.execute(task_session).await;
        }
    });
    session_arc.lock().await.data_task = Some(task);
    let _ = start_tx.send(());
}

struct HumanDuration(Duration);

impl fmt::Display for HumanDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let total_secs = self.0.as_secs();

        let hours = total_secs / 3600;
        let minutes = (total_secs % 3600) / 60;
        let seconds = total_secs % 60;
        let millis = self.0.subsec_millis();

        if hours > 0 {
            write!(f, "{}h {}m {}s {}ms", hours, minutes, seconds, millis)
        } else if minutes > 0 {
            write!(f, "{}m {}s {}ms", minutes, seconds, millis)
        } else if seconds > 0 {
            write!(f, "{}s {}ms", seconds, millis)
        } else {
            write!(f, "{}ms", millis)
        }
    }
}

struct HumanBytes(u64);

impl fmt::Display for HumanBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const KIB: u64 = 1024;
        const MIB: u64 = KIB * 1024;
        const GIB: u64 = MIB * 1024;
        const TIB: u64 = GIB * 1024;

        if self.0 >= TIB {
            write!(f, "{:.2} TiB", (self.0 as f64) / (TIB as f64))
        } else if self.0 >= GIB {
            write!(f, "{:.2} GiB", (self.0 as f64) / (GIB as f64))
        } else if self.0 >= MIB {
            write!(f, "{:.2} MiB", (self.0 as f64) / (MIB as f64))
        } else if self.0 >= KIB {
            write!(f, "{:.2} KiB", (self.0 as f64) / (KIB as f64))
        } else {
            write!(f, "{} B", self.0)
        }
    }
}

struct TransferSpeed(f64);

impl fmt::Display for TransferSpeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kb_per_second = self.0 / 1024.0;
        if kb_per_second < 1.0 {
            return write!(f, "{:.2} B/s", self.0);
        }

        let mb_per_second = kb_per_second / 1024.0;
        if mb_per_second < 1.0 {
            return write!(f, "{:.2} KB/s", kb_per_second);
        }

        let gb_per_second = mb_per_second / 1024.0;
        if gb_per_second < 1.0 {
            return write!(f, "{:.2} MB/s", mb_per_second);
        }

        write!(f, "{:.2} GB/s", gb_per_second)
    }
}

// Collapse the StorageError kind into a client-error, server-error or unknown-error.
// The PermissionDenied is seperated because it depends on specifics whether it is a server or client error
// Unknown errors should not happen but need to be handled
fn categorize_and_register_error(logger: &slog::Logger, err: &Error, command: &'static str) {
    match err.kind() {
        ErrorKind::PermanentFileNotAvailable => metrics::inc_transferred(command, "client-error"),
        ErrorKind::TransientFileNotAvailable | ErrorKind::LocalError => metrics::inc_transferred(command, "server-error"),
        ErrorKind::PermissionDenied => metrics::inc_transferred(command, "permission-error"),
        ErrorKind::ConnectionClosed => {
            if let Some(io_error) = err.get_io_error() {
                match io_error.kind() {
                    std::io::ErrorKind::ConnectionReset => metrics::inc_transferred(command, "client-interrupted"),
                    std::io::ErrorKind::BrokenPipe => {
                        // Clients like Cyberduck appear to close the connection prematurely for chunked downloading, generating many "errors"
                        if command != "retr" {
                            metrics::inc_transferred(command, "client-interrupted");
                        }
                    }
                    std::io::ErrorKind::ConnectionAborted => metrics::inc_transferred(command, "network-error"), // Could be a network issue
                    _ => {
                        let log_error = sanitize_control_text(&format!("{io_error:?}"));
                        slog::debug!(logger, "Unmapped ConnectionClosed io error: {}", log_error);
                        metrics::inc_transferred(command, "server-error")
                    }
                }
            }
        }
        _ => {
            let log_error = sanitize_control_text(&format!("{err:?}"));
            slog::debug!(logger, "Unmapped error: {}", log_error);
            metrics::inc_transferred(command, "unknown-error")
        }
    }
}

#[derive(Debug)]
enum ListCommand {
    List,
    Nlst,
}

impl ListCommand {
    fn as_str(&self) -> &'static str {
        match self {
            ListCommand::List => "LIST",
            ListCommand::Nlst => "NLST",
        }
    }
    fn as_lower_str(&self) -> &'static str {
        match self {
            ListCommand::List => "list",
            ListCommand::Nlst => "nlst",
        }
    }
}
