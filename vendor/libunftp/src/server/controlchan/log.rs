use crate::server::{
    Command, Event, Reply,
    controlchan::{error::ControlChanError, middleware::ControlChanMiddleware},
};
use slog::Drain;

use async_trait::async_trait;

// Control channel middleware that logs all control channel events
pub struct LoggingMiddleware<Next>
where
    Next: ControlChanMiddleware,
{
    pub logger: slog::Logger,
    pub sequence_nr: u64,
    pub next: Next,
}

#[async_trait]
impl<Next> ControlChanMiddleware for LoggingMiddleware<Next>
where
    Next: ControlChanMiddleware,
{
    async fn handle(&mut self, event: Event) -> Result<Reply, ControlChanError> {
        self.sequence_nr += 1;
        if let Event::Command(Command::User { username }) = &event {
            let username = String::from_utf8_lossy(username);
            let s = super::sanitize_control_text(&username);
            self.logger = self.logger.new(slog::o!("username" => s));
        }
        if self.logger.is_enabled(slog::Level::Debug) {
            let event_text = super::sanitize_control_text(&format!("{event:?}"));
            slog::debug!(self.logger, "Control channel event {}", event_text; "seq" => self.sequence_nr);
        }
        let result = self.next.handle(event).await;
        match &result {
            Ok(reply) if self.logger.is_enabled(slog::Level::Debug) => {
                let reply_text = super::sanitize_control_text(&format!("{reply:?}"));
                slog::debug!(self.logger, "Control channel reply {}", reply_text; "seq" => self.sequence_nr);
            }
            Err(error) if self.logger.is_enabled(slog::Level::Warning) => {
                let error_text = super::sanitize_control_text(&format!("{error:?}"));
                slog::warn!(self.logger, "Control channel error {}", error_text; "seq" => self.sequence_nr);
            }
            _ => {}
        };
        result
    }
}
