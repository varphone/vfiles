//! The RFC 959 Abort (`ABOR`) command
//
// This command tells the server to abort the previous FTP
// service command and any associated transfer of data. The
// abort command may require "special action", as discussed in
// the Section on FTP Commands, to force recognition by the
// server.  No action is to be taken if the previous command
// has been completed (including data transfer).  The control
// connection is not to be closed by the server, but the data
// connection must be closed.

use crate::server::controlchan::{
    Reply, ReplyCode,
    error::ControlChanError,
    handler::{CommandContext, CommandHandler},
};
use async_trait::async_trait;
use unftp_core::auth::UserDetail;
use unftp_core::storage::{Metadata, StorageBackend};

#[derive(Debug)]
pub struct Abor;

#[async_trait]
impl<Storage, User> CommandHandler<Storage, User> for Abor
where
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    User: UserDetail + 'static,
{
    #[tracing_attributes::instrument(skip_all)]
    async fn handle(&self, args: CommandContext<Storage, User>) -> Result<Reply, ControlChanError> {
        let mut session = args.session.lock().await;
        match session.data_abort_tx.take() {
            Some(token) => {
                token.cancel();
                Ok(Reply::new(ReplyCode::ClosingDataConnection, "Closed data channel"))
            }
            None => Ok(Reply::new(ReplyCode::ClosingDataConnection, "Data channel already closed")),
        }
    }
}
