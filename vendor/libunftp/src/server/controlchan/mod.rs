//! Contains code pertaining to the FTP *control* channel/connection.

pub mod command;

pub(crate) mod event;
pub(crate) mod handler;
pub(crate) mod reply;

pub(super) mod commands;

mod active_passive;
mod auth;
mod codecs;
mod control_loop;
mod error;
mod ftps;
mod line_parser;
mod log;
mod middleware;
mod notify;

use command::Command;
pub(crate) use control_loop::{Config as LoopConfig, spawn as spawn_loop};
pub(crate) use error::{ControlChanError, ControlChanErrorKind};
pub(crate) use event::Event;
pub(crate) use middleware::ControlChanMiddleware;
pub(crate) use reply::{Reply, ReplyCode};

pub(crate) fn is_unsafe_log_char(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '\u{2028}'
                | '\u{2029}'
                | '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        )
}

pub(crate) fn sanitize_control_text(text: &str) -> String {
    text.chars().map(|ch| if is_unsafe_log_char(ch) { ' ' } else { ch }).collect()
}

pub(crate) fn sanitize_control_path(path: &std::path::Path) -> String {
    sanitize_control_text(&path.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::{sanitize_control_path, sanitize_control_text};

    #[test]
    fn log_text_replaces_control_and_bidirectional_formatting_characters() {
        let text = "USER\r\nname\u{202e}hidden\u{2066}value\u{2069}\u{001b}";
        assert_eq!(sanitize_control_text(text), "USER  name hidden value  ");
    }

    #[test]
    fn log_paths_replace_bidirectional_formatting_characters() {
        let path = std::path::Path::new("reports/\u{202e}txt.exe");
        assert_eq!(sanitize_control_path(path), "reports/ txt.exe");
    }
}
