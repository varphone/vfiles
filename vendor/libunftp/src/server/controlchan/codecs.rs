use super::{
    Reply,
    command::Command,
    error::{ControlChanError, ControlChanErrorKind},
    line_parser,
};

use bytes::BytesMut;
use std::io::Write;
use tokio_util::codec::{Decoder, Encoder};

// VFiles patch: bound control-channel framing so a peer cannot grow the line buffer indefinitely.
const MAX_COMMAND_LINE_BYTES: usize = 8 * 1024;

// FTPCodec implements tokio's `Decoder` and `Encoder` traits for the control channel, that we'll
// use to decode FTP commands and encode their responses.
pub struct FtpCodec {
    // Stored index of the next index to examine for a '\n' character. This is used to optimize
    // searching. For example, if `decode` was called with `abc`, it would hold `3`, because that
    // is the next index to examine. The next time `decode` is called with `abcde\n`, we will only
    // look at `de\n` before returning.
    next_index: usize,
}

impl FtpCodec {
    pub fn new() -> Self {
        FtpCodec { next_index: 0 }
    }
}

impl Decoder for FtpCodec {
    type Item = Command;
    type Error = ControlChanError;

    // Here we decode the incoming bytes into a meaningful command. We'll split on newlines, and
    // parse the resulting line using `Command::parse()`. This method will be called by tokio.
    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Command>, Self::Error> {
        if let Some(newline_offset) = buf[self.next_index..].iter().position(|b| *b == b'\n') {
            let newline_index = newline_offset + self.next_index;
            if newline_index + 1 > MAX_COMMAND_LINE_BYTES {
                return Err(ControlChanErrorKind::ParseError.into());
            }
            let line = buf.split_to(newline_index + 1);
            self.next_index = 0;
            Ok(Some(line_parser::parse(line)?))
        } else {
            if buf.len() > MAX_COMMAND_LINE_BYTES {
                return Err(ControlChanErrorKind::ParseError.into());
            }
            self.next_index = buf.len();
            Ok(None)
        }
    }
}

impl Encoder<Reply> for FtpCodec {
    type Error = ControlChanError;

    // Here we encode the outgoing response
    fn encode(&mut self, reply: Reply, buf: &mut BytesMut) -> Result<(), Self::Error> {
        let mut buffer = vec![];
        match reply {
            Reply::None => {
                return Ok(());
            }
            Reply::CodeAndMsg { code, msg } => {
                let msg = super::sanitize_control_text(&msg);
                if msg.is_empty() {
                    writeln!(buffer, "{}\r", code as u32)?;
                } else {
                    writeln!(buffer, "{} {}\r", code as u32, msg)?;
                }
            }
            Reply::MultiLine { code, mut lines } => {
                for line in &mut lines {
                    *line = super::sanitize_control_text(line);
                }
                // Get the last line since it needs to be preceded by the response code.
                let last_line = lines.pop().unwrap_or_default();

                // Lines starting with a digit should be indented
                for it in lines.iter_mut() {
                    if it.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
                        it.insert(0, ' ');
                    }
                }
                if lines.is_empty() {
                    writeln!(buffer, "{} {}\r", code as u32, last_line)?;
                } else {
                    write!(buffer, "{}-{}\r\n{} {}\r\n", code as u32, lines.join("\r\n"), code as u32, last_line)?;
                }
            }
        }
        buf.extend(&buffer);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_limit_accepts_the_boundary_and_rejects_oversized_frames() {
        let max_username_len = MAX_COMMAND_LINE_BYTES - b"USER \r\n".len();
        let maximum_line = format!("USER {}\r\n", "a".repeat(max_username_len));
        let mut codec = FtpCodec::new();
        let mut frame = BytesMut::from(maximum_line.as_bytes());

        let command = codec.decode(&mut frame).expect("maximum frame should decode");
        assert!(matches!(command, Some(Command::User { username }) if username.len() == max_username_len));
        assert!(frame.is_empty());

        let oversized_line = format!("USER {}\r\n", "a".repeat(max_username_len + 1));
        let mut codec = FtpCodec::new();
        let mut frame = BytesMut::from(oversized_line.as_bytes());
        let error = codec.decode(&mut frame).expect_err("oversized frame should be rejected");
        assert_eq!(error.kind(), &ControlChanErrorKind::ParseError);

        let mut codec = FtpCodec::new();
        let mut frame = BytesMut::from(&oversized_line.as_bytes()[..MAX_COMMAND_LINE_BYTES]);
        assert!(codec.decode(&mut frame).expect("partial frame should wait").is_none());
        frame.extend_from_slice(&oversized_line.as_bytes()[MAX_COMMAND_LINE_BYTES..]);
        let error = codec.decode(&mut frame).expect_err("fragmented oversized frame should be rejected");
        assert_eq!(error.kind(), &ControlChanErrorKind::ParseError);
    }

    #[test]
    fn reply_text_cannot_inject_control_channel_lines() {
        let mut codec = FtpCodec::new();
        let mut single_line = BytesMut::new();
        codec
            .encode(Reply::new(super::super::ReplyCode::CommandOkay, "ok\r\n230 forged"), &mut single_line)
            .expect("single-line reply should encode");
        assert_eq!(&single_line[..], b"200 ok  230 forged\r\n");

        let mut multiline = BytesMut::new();
        codec
            .encode(
                Reply::new_multiline(super::super::ReplyCode::SystemStatus, ["211\r\n230 forged", "done"]),
                &mut multiline,
            )
            .expect("multiline reply should encode");
        assert_eq!(&multiline[..], b"211- 211  230 forged\r\n211 done\r\n");
    }
}
