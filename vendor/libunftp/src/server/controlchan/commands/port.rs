//! The RFC 959 Data Port (`PORT`) command
//
// The argument is a HOST-PORT specification for the data port
// to be used in data connection.  There are defaults for both
// the user and server data ports, and under normal
// circumstances this command and its reply are not needed.  If
// this command is used, the argument is the concatenation of a
// 32-bit internet host address and a 16-bit TCP port address.
// This address information is broken into 8-bit fields and the
// value of each field is transmitted as a decimal number (in
// character string representation).  The fields are separated
// by commas.  A port command would be:
//
// PORT h1,h2,h3,h4,p1,p2
//
// where h1 is the high order 8 bits of the internet host
// address.

use super::passive_common::cancel_legacy_passive_listener;
use crate::{
    auth::UserDetail,
    server::{
        ControlChanMsg,
        chancomms::DataChanCmd,
        controlchan::{
            Reply, ReplyCode,
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
        },
        datachan,
        session::SharedSession,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use std::net::{IpAddr, Ipv4Addr, SocketAddrV4};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio_util::sync::CancellationToken;

const ACTIVE_DATA_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Debug)]
pub struct Port {
    addr: String,
}

impl Port {
    pub fn new(addr: String) -> Self {
        Port { addr }
    }

    fn parse_address(addr: &str) -> Option<SocketAddrV4> {
        let mut parts = addr.split(',');
        let mut octets = [0_u8; 6];
        for octet in &mut octets {
            let part = parts.next()?;
            if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            *octet = part.parse().ok()?;
        }
        if parts.next().is_some() {
            return None;
        }

        let port = (u16::from(octets[4]) << 8) | u16::from(octets[5]);
        Some(SocketAddrV4::new(Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3]), port))
    }

    fn matches_control_peer(control_peer: IpAddr, target: Ipv4Addr) -> bool {
        match control_peer {
            IpAddr::V4(peer) => peer == target,
            IpAddr::V6(peer) => peer.to_ipv4_mapped() == Some(target),
        }
    }

    // modifies the session by adding channels that are used to communicate with the data connection
    // processing loop.
    #[tracing_attributes::instrument]
    async fn setup_inter_loop_comms<S, U>(&self, session: SharedSession<S, U>, control_loop_tx: Sender<ControlChanMsg>)
    where
        U: UserDetail + 'static,
        S: StorageBackend<U> + 'static,
        S::Metadata: Metadata,
    {
        cancel_legacy_passive_listener(session.clone()).await;
        let (cmd_tx, cmd_rx): (Sender<DataChanCmd>, Receiver<DataChanCmd>) = channel(1);
        let data_abort_tx = CancellationToken::new();

        let mut session = session.lock().await;
        session.data_cmd_tx = Some(cmd_tx);
        session.data_cmd_rx = Some(cmd_rx);
        session.data_abort_tx = Some(data_abort_tx);
        session.control_msg_tx = Some(control_loop_tx);
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Port
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let CommandContext {
            logger,
            tx_control_chan: tx,
            session,
            ..
        } = args;

        let Some(addr) = Self::parse_address(&self.addr) else {
            return Ok(Reply::new(ReplyCode::ParameterSyntaxError, "Invalid address format"));
        };

        let control_peer_ip = session.lock().await.source.ip();
        if !Self::matches_control_peer(control_peer_ip, *addr.ip()) {
            slog::debug!(
                logger,
                "Rejecting active data connection to {:?}: target IP does not match control peer {:?}",
                addr,
                control_peer_ip
            );
            return Ok(Reply::new(
                ReplyCode::CantOpenDataConnection,
                "Active data address must match the control connection",
            ));
        }

        let stream = match tokio::time::timeout(ACTIVE_DATA_CONNECT_TIMEOUT, TcpStream::connect(addr)).await {
            Ok(Ok(stream)) => stream,
            Ok(Err(err)) => {
                slog::warn!(logger, "Could not connect to client for active mode: {}", err);
                return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "Could not establish data connection"));
            }
            Err(_) => {
                slog::warn!(logger, "Timed out connecting to client for active data connection");
                return Ok(Reply::new(ReplyCode::CantOpenDataConnection, "Active data connection timed out"));
            }
        };

        self.setup_inter_loop_comms(session.clone(), tx).await;
        datachan::spawn_processing(logger, session, stream).await;

        Ok(Reply::new(ReplyCode::CommandOkay, "PORT command successful"))
    }
}
