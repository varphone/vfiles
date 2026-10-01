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
