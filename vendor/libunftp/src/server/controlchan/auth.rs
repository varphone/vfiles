use crate::{
    auth::UserDetail,
    server::{
        controlchan::{error::ControlChanError, middleware::ControlChanMiddleware},
        session::SharedSession,
        {Command, Event, Reply, ReplyCode, SessionState},
    },
    storage::{Metadata, StorageBackend},
};

use crate::server::controlchan::commands::Opt;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

const USER_REVALIDATION_INTERVAL: Duration = Duration::from_secs(5);

// AuthMiddleware ensures the user is authenticated before he can do much else.
pub struct AuthMiddleware<Storage, User, Next>
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    Next: ControlChanMiddleware,
{
    pub session: SharedSession<Storage, User>,
    pub next: Next,
    pub(super) last_revalidation: Option<tokio::time::Instant>,
}

#[async_trait]
impl<Storage, User, Next> ControlChanMiddleware for AuthMiddleware<Storage, User, Next>
where
    User: UserDetail + 'static,
    Storage: StorageBackend<User> + 'static,
    Storage::Metadata: Metadata,
    Next: ControlChanMiddleware,
{
    async fn handle(&mut self, event: Event) -> Result<Reply, ControlChanError> {
        if matches!(event, Event::InternalMsg(_)) {
            return self.next.handle(event).await;
        }

        let (session_state, storage, user) = {
            let session = self.session.lock().await;
            (
                session.state,
                Arc::clone(&session.storage),
                Arc::clone(&session.user),
            )
        };
        let is_quit = matches!(&event, Event::Command(Command::Quit));
        if session_state == SessionState::WaitCmd
            && !is_quit
            && self.last_revalidation.is_none_or(|last| {
                last.elapsed() >= USER_REVALIDATION_INTERVAL
            })
        {
            let Some(user) = user.as_ref() else {
                return Err(ControlChanError::new(
                    crate::server::controlchan::error::ControlChanErrorKind::IllegalState,
                ));
            };
            storage.revalidate_user(user).await.map_err(|_| {
                ControlChanError::new(
                    crate::server::controlchan::error::ControlChanErrorKind::AuthenticationError,
                )
            })?;
            self.last_revalidation = Some(tokio::time::Instant::now());
        }

        match event {
            // internal messages and the below commands are exempt from auth checks.
            Event::Command(Command::Help)
            | Event::Command(Command::User { .. })
            | Event::Command(Command::Pass { .. })
            | Event::Command(Command::Auth { .. })
            | Event::Command(Command::Prot { .. })
            | Event::Command(Command::Pbsz { .. })
            | Event::Command(Command::Feat)
            | Event::Command(Command::Noop)
            | Event::Command(Command::Opts { option: Opt::Utf8 { .. } })
            | Event::Command(Command::Quit) => self.next.handle(event).await,
            _ => {
                let session_state = async {
                    let session = self.session.lock().await;
                    session.state
                }
                .await;
                if session_state != SessionState::WaitCmd {
                    Ok(Reply::new(ReplyCode::NotLoggedIn, "Please authenticate"))
                } else {
                    self.next.handle(event).await
                }
            }
        }
    }
}
