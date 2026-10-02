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

    fn validate_address(addr: &str, control_peer: IpAddr) -> Result<SocketAddrV4, PortAddressError> {
        let addr = Self::parse_address(addr).ok_or(PortAddressError::InvalidFormat)?;
        if !Self::matches_control_peer(control_peer, *addr.ip()) {
            return Err(PortAddressError::PeerMismatch(addr));
        }
        Ok(addr)
    }

    // modifies the session by adding channels that are used to communicate with the data connection
    // processing loop.
    #[tracing_attributes::instrument(skip_all)]
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
    #[tracing_attributes::instrument(skip_all)]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let CommandContext {
            logger,
            tx_control_chan: tx,
            session,
            ..
        } = args;

        let control_peer_ip = session.lock().await.source.ip();
        let addr = match Self::validate_address(&self.addr, control_peer_ip) {
            Ok(addr) => addr,
            Err(PortAddressError::InvalidFormat) => {
                return Ok(Reply::new(ReplyCode::ParameterSyntaxError, "Invalid address format"));
            }
            Err(PortAddressError::PeerMismatch(addr)) => {
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
        };

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

#[derive(Debug, PartialEq, Eq)]
enum PortAddressError {
    InvalidFormat,
    PeerMismatch(SocketAddrV4),
}

#[cfg(test)]
mod tests {
    use super::{Port, PortAddressError};
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn port_accepts_only_a_well_formed_address_for_the_control_peer() {
        let peer = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 4));
        let address = Port::validate_address("192,0,2,4,7,138", peer).expect("client address should be accepted");
        assert_eq!(address.ip(), &Ipv4Addr::new(192, 0, 2, 4));
        assert_eq!(address.port(), 1930);

        let mapped_peer = IpAddr::V6(Ipv6Addr::from((0xffff_u128 << 32) | u128::from(Ipv4Addr::new(192, 0, 2, 4).to_bits())));
        assert!(Port::validate_address("192,0,2,4,7,138", mapped_peer).is_ok());
    }

    #[test]
    fn port_rejects_third_party_and_malformed_targets_before_connecting() {
        let control_peer = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 4));
        assert!(
            matches!(
                Port::validate_address("198,51,100,9,7,138", control_peer),
                Err(PortAddressError::PeerMismatch(_))
            ),
            "PORT must not authorize a connection to another host"
        );

        for malformed in ["192,0,2,4,7", "192,0,2,4,7,138,1", "192,0,2,256,7,138", "192,0,2,-1,7,138", "192.0.2.4,7,138"] {
            assert_eq!(
                Port::validate_address(malformed, control_peer),
                Err(PortAddressError::InvalidFormat),
                "invalid PORT target should be rejected: {malformed}"
            );
        }
    }
}
