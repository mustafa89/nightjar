//! The link to the notch app (NotchSwitcher), over UDP on localhost only.
//!
//! - Nightjar → notch, port 47811: a JSON status (state, title, artist, position, length,
//!   spectrum bands, skin accents), 30 times a second while playing, once a second otherwise.
//!   Nobody listening is fine: the packets are dropped.
//! - Notch → Nightjar, port 47812: `toggle`, `next` or `prev` as plain text.

use std::net::UdpSocket;
use std::sync::mpsc::{Receiver, channel};

pub const STATUS_PORT: u16 = 47811;
pub const COMMAND_PORT: u16 = 47812;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Next,
    Prev,
}

impl Command {
    pub fn parse(s: &str) -> Option<Command> {
        match s.trim() {
            "toggle" => Some(Command::Toggle),
            "next" => Some(Command::Next),
            "prev" => Some(Command::Prev),
            _ => None,
        }
    }
}

pub struct Bridge {
    out: Option<UdpSocket>,
    rx: Receiver<Command>,
}

impl Bridge {
    /// Opens the status socket and, unless another Nightjar holds the port, listens for commands.
    pub fn start(waker: Option<rune::Waker>) -> Bridge {
        let out = UdpSocket::bind(("127.0.0.1", 0))
            .ok()
            .filter(|s| s.connect(("127.0.0.1", STATUS_PORT)).is_ok());
        let (tx, rx) = channel();
        if let Ok(sock) = UdpSocket::bind(("127.0.0.1", COMMAND_PORT)) {
            let _ = std::thread::Builder::new()
                .name("nightjar-bridge".into())
                .spawn(move || {
                    let mut buf = [0u8; 64];
                    while let Ok((n, _)) = sock.recv_from(&mut buf) {
                        let Some(cmd) =
                            std::str::from_utf8(&buf[..n]).ok().and_then(Command::parse)
                        else {
                            continue;
                        };
                        if tx.send(cmd).is_err() {
                            break;
                        }
                        if let Some(w) = &waker {
                            w();
                        }
                    }
                });
        }
        Bridge { out, rx }
    }

    pub fn commands(&self) -> impl Iterator<Item = Command> + '_ {
        self.rx.try_iter()
    }

    pub fn send(&self, status: &serde_json::Value) {
        if let (Some(sock), Ok(bytes)) = (&self.out, serde_json::to_vec(status)) {
            // ECONNREFUSED while the notch app isn't running: expected, ignore.
            let _ = sock.send(&bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_parse_and_junk_is_ignored() {
        assert_eq!(Command::parse("toggle\n"), Some(Command::Toggle));
        assert_eq!(Command::parse("next"), Some(Command::Next));
        assert_eq!(Command::parse("prev"), Some(Command::Prev));
        assert_eq!(Command::parse("rm -rf"), None);
    }
}
