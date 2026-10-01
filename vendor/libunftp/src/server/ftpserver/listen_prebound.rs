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
