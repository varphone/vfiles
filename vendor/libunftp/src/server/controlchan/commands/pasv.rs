//! The RFC 959 Passive (`PASV`) command
//
// This command requests the server-DTP to "listen" on a data
// port (which is not its default data port) and to wait for a
// connection rather than initiate one upon receipt of a
// transfer command.  The response to this command includes the
// host and port address this server is listening on.

use crate::{
    auth::UserDetail,
    server::{
        ControlChanErrorKind,
        chancomms::SwitchboardSender,
        controlchan::{
            Reply, ReplyCode,
            error::ControlChanError,
            handler::{CommandContext, CommandHandler},
        },
        ftpserver::options::PassiveHost,
    },
    storage::{Metadata, StorageBackend},
};
use async_trait::async_trait;
use std::{net::Ipv4Addr, time::Duration};

use super::passive_common::{self, LegacyReplyProducer};

const PASV_DNS_LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct Pasv {}

impl Pasv {
    pub fn new() -> Self {
        Pasv {}
    }
}

#[async_trait]
impl<Storage, User> LegacyReplyProducer<Storage, User> for Pasv
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
{
    async fn build_reply(&self, args: &CommandContext<Storage, User>, port: u16) -> Result<Reply, ControlChanError> {
        let conn_addr = match args.local_addr {
            std::net::SocketAddr::V4(addr) => *addr.ip(),
            std::net::SocketAddr::V6(_) => {
                slog::error!(args.logger, "local address is ipv6! we only listen on ipv4, so this shouldn't happen");
                return Err(ControlChanErrorKind::InternalServerError.into());
            }
        };
        Ok(make_pasv_reply(&args.logger, args.passive_host.clone(), &conn_addr, port).await)
    }
}

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Pasv
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
{
    #[tracing_attributes::instrument]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let sender: Option<SwitchboardSender<Storage, User>> = args.tx_prebound_loop.clone();
        match sender {
            Some(tx) => passive_common::handle_delegated_mode(args, tx).await,
            None => passive_common::handle_legacy_mode(self, args).await,
        }
    }
}

pub async fn make_pasv_reply(logger: &slog::Logger, passive_host: PassiveHost, conn_ip: &Ipv4Addr, port: u16) -> Reply {
    let p1 = port >> 8;
    let p2 = port - (p1 * 256);
    let octets = match passive_host {
        PassiveHost::Ip(ip) => ip.octets(),
        PassiveHost::FromConnection => conn_ip.octets(),
        PassiveHost::Dns(ref dns_name) => {
            let dns_host = dns_name.split(':').next().unwrap_or(dns_name.as_str());
            let lookup = tokio::time::timeout(PASV_DNS_LOOKUP_TIMEOUT, tokio::net::lookup_host(format!("{dns_host}:2121"))).await;
            match lookup {
                Err(_) => {
                    slog::warn!(logger, "make_pasv_reply: Timed out looking up host for pasv reply");

                    return Reply::new_with_string(ReplyCode::CantOpenDataConnection, format!("Timed out resolving DNS address '{}'", dns_name));
                }
                Ok(Err(err)) => {
                    slog::warn!(logger, "make_pasv_reply: Could not look up host for pasv reply: {}", err);

                    return Reply::new_with_string(ReplyCode::CantOpenDataConnection, format!("Could not resolve DNS address '{}'", dns_name));
                }
                Ok(Ok(mut ip_iter)) => loop {
                    match ip_iter.next() {
                        None => return Reply::new_with_string(ReplyCode::CantOpenDataConnection, format!("Could not resolve DNS address '{}'", dns_name)),
                        Some(std::net::SocketAddr::V4(ip)) => break ip.ip().octets(),
                        Some(std::net::SocketAddr::V6(_)) => continue,
                    }
                },
            }
        }
    };
    slog::info!(logger, "Listening on passive port {}:{}", conn_ip, port);
    Reply::new_with_string(
        ReplyCode::EnteringPassiveMode,
        format!("Entering Passive Mode ({},{},{},{},{},{})", octets[0], octets[1], octets[2], octets[3], p1, p2),
    )
}
