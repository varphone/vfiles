//! This module contains the implementations for the FTP commands defined in
//!
//! - [RFC 959 - FTP](https://tools.ietf.org/html/rfc959)
//! - [RFC 3659 - Extensions to FTP](https://tools.ietf.org/html/rfc3659)
//! - [RFC 2228 - FTP Security Extensions](https://tools.ietf.org/html/rfc2228)

use std::future::Future;

use crate::server::chancomms::ControlChanMsg;
use tokio::sync::mpsc::Sender;

mod abor;
mod acct;
mod allo;
mod appe;
mod auth;
mod ccc;
mod cdup;
mod cwd;
mod dele;
mod epsv;
mod feat;
mod help;
mod list;
mod md5;
mod mdtm;
mod mkd;
mod mlsd;
pub(crate) mod mlst;
mod mode;
mod nlst;
mod noop;
mod opts;
mod pass;
pub(crate) mod passive_common;
mod pasv;
mod pbsz;
mod port;
mod prot;
mod pwd;
mod quit;
mod rest;
mod retr;
mod rmd;
mod rnfr;
mod rnto;
mod size;
mod stat;
mod stor;
mod stou;
mod stru;
mod syst;
mod type_;
mod user;

pub use self::md5::Md5;
pub use abor::Abor;
pub use acct::Acct;
pub use allo::Allo;
pub use appe::Appe;
pub use auth::{Auth, AuthParam};
pub use ccc::Ccc;
pub use cdup::Cdup;
pub use cwd::Cwd;
pub use dele::Dele;
pub use epsv::Epsv;
pub use feat::Feat;
pub use help::Help;
pub use list::List;
pub use mdtm::Mdtm;
pub use mkd::Mkd;
pub use mlsd::Mlsd;
pub use mlst::Mlst;
pub use mode::{Mode, ModeParam};
pub use nlst::Nlst;
pub use noop::Noop;
pub use opts::{Opt, Opts};
pub use pass::Pass;
pub use pasv::Pasv;
pub use pasv::make_pasv_reply;
pub use pbsz::Pbsz;
pub use port::Port;
pub use prot::{Prot, ProtParam};
pub use pwd::Pwd;
pub use quit::Quit;
pub use rest::Rest;
pub use retr::Retr;
pub use rmd::Rmd;
pub use rnfr::Rnfr;
pub use rnto::Rnto;
pub use size::Size;
pub use stat::Stat;
pub use stor::Stor;
pub use stou::Stou;
pub use stru::{Stru, StruParam};
pub use syst::Syst;
pub use type_::Type;
pub use user::User;

/// Run detached control-command work only while its control session is still open.
pub(crate) async fn while_control_channel_open<F>(tx: &Sender<ControlChanMsg>, future: F) -> Option<F::Output>
where
    F: Future + Send,
{
    tokio::select! {
        result = future => Some(result),
        _ = tx.closed() => None,
    }
}

#[cfg(test)]
mod tests {
    use super::while_control_channel_open;
    use std::future::pending;
    use tokio::sync::{mpsc::channel, oneshot};

    #[tokio::test]
    async fn control_channel_close_cancels_detached_storage_work() {
        let (control_tx, control_rx) = channel(1);
        let (started_tx, started_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            while_control_channel_open(&control_tx, async move {
                let _ = started_tx.send(());
                pending::<()>().await;
            })
            .await
        });

        started_rx.await.expect("storage future should start");
        drop(control_rx);
        let result = tokio::time::timeout(std::time::Duration::from_millis(100), task)
            .await
            .expect("closed control channel should cancel the pending future")
            .expect("command task should finish cleanly");
        assert!(result.is_none(), "cancelled storage work must not report a result");
    }
}
