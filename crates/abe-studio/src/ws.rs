//! WebSocket (RFC 6455) for the edit protocol: the opening handshake, and reading and writing
//! text messages over any byte stream. Messages are bounded; fragmented messages are joined;
//! pings are answered; a close frame ends the connection.
use std::io::{Read, Write};

use abe_core::digest::{base64, sha1};

use crate::http::{Request, Response};

/// Largest message the host accepts or sends (a whole scene document travels as one message).
pub const MAX_MESSAGE: usize = 64 * 1024 * 1024;
const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// The `101 Switching Protocols` answer to an upgrade request, or why it is not one.
pub fn accept(req: &Request) -> Result<Response, String> {
    let upgrade = req
        .header("upgrade")
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    let connection = req.header("connection").is_some_and(|v| {
        v.split(',')
            .any(|t| t.trim().eq_ignore_ascii_case("upgrade"))
    });
    if req.method != "GET" || !upgrade || !connection {
        return Err("not a WebSocket upgrade".into());
    }
    if req.header("sec-websocket-version") != Some("13") {
        return Err("WebSocket version 13 required".into());
    }
    let key = req
        .header("sec-websocket-key")
        .ok_or("missing Sec-WebSocket-Key")?;
    let digest = sha1(format!("{key}{GUID}").as_bytes());
    Ok(Response {
        status: 101,
        headers: vec![
            ("Upgrade".into(), "websocket".into()),
            ("Connection".into(), "Upgrade".into()),
            ("Sec-WebSocket-Accept".into(), base64(&digest)),
        ],
        body: vec![],
    })
}

#[derive(Debug, PartialEq)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    Close,
}

fn read_exact(r: &mut impl Read, n: usize) -> Result<Vec<u8>, String> {
    let mut b = vec![0; n];
    r.read_exact(&mut b).map_err(|e| e.to_string())?;
    Ok(b)
}

/// Reads the next message from a client (whose frames must be masked). A ping met on the way is
/// answered at once: `pong` receives the complete pong frame to send.
pub fn read_message(r: &mut impl Read, pong: &mut dyn FnMut(Vec<u8>)) -> Result<Message, String> {
    let mut data = Vec::new();
    let mut kind = None;
    loop {
        let head = read_exact(r, 2)?;
        let fin = head[0] & 0x80 != 0;
        if head[0] & 0x70 != 0 {
            return Err("reserved bits set".into());
        }
        let opcode = head[0] & 0x0f;
        let masked = head[1] & 0x80 != 0;
        if !masked {
            return Err("client frames must be masked".into());
        }
        let mut len = u64::from(head[1] & 0x7f);
        if len == 126 {
            let b = read_exact(r, 2)?;
            len = u64::from(u16::from_be_bytes([b[0], b[1]]));
        } else if len == 127 {
            let b = read_exact(r, 8)?;
            len = u64::from_be_bytes(b.try_into().expect("8 bytes"));
        }
        if len > MAX_MESSAGE as u64 || data.len() as u64 + len > MAX_MESSAGE as u64 {
            return Err("message too large".into());
        }
        let mask = read_exact(r, 4)?;
        let mut payload = read_exact(r, len as usize)?;
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= mask[i % 4];
        }
        match opcode {
            0x8 => return Ok(Message::Close),
            0x9 => {
                if !fin || payload.len() > 125 {
                    return Err("bad ping".into());
                }
                pong(frame(0xA, &payload));
                continue;
            }
            0xA => continue,
            0x1 | 0x2 => {
                if kind.is_some() {
                    return Err("new message inside a fragmented one".into());
                }
                kind = Some(opcode);
            }
            0x0 => {
                if kind.is_none() {
                    return Err("continuation without a message".into());
                }
            }
            _ => return Err(format!("unknown opcode {opcode}")),
        }
        data.extend_from_slice(&payload);
        if fin {
            return match kind {
                Some(0x1) => String::from_utf8(data)
                    .map(Message::Text)
                    .map_err(|_| "text message is not UTF-8".into()),
                _ => Ok(Message::Binary(data)),
            };
        }
    }
}

/// One whole unmasked frame (servers never mask): header and payload in one buffer.
pub fn frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(payload.len() + 10);
    f.push(0x80 | opcode);
    match payload.len() {
        n if n < 126 => f.push(n as u8),
        n if n <= 0xffff => {
            f.push(126);
            f.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            f.push(127);
            f.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    f.extend_from_slice(payload);
    f
}

/// Writes one frame in a single write. Written as two, the payload would wait (Nagle's
/// algorithm) until the peer acknowledged the header, which a peer delays by up to 40 ms or
/// more when it has nothing to send back.
pub fn write_frame(w: &mut impl Write, opcode: u8, payload: &[u8]) -> std::io::Result<()> {
    w.write_all(&frame(opcode, payload))?;
    w.flush()
}

pub fn write_text(w: &mut impl Write, text: &str) -> std::io::Result<()> {
    write_frame(w, 0x1, text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client_frame(opcode: u8, fin: bool, payload: &[u8]) -> Vec<u8> {
        let mask = [0x37, 0xfa, 0x21, 0x3d];
        let mut f = vec![if fin { 0x80 } else { 0 } | opcode];
        match payload.len() {
            n if n < 126 => f.push(0x80 | n as u8),
            n if n <= 0xffff => {
                f.push(0x80 | 126);
                f.extend_from_slice(&(n as u16).to_be_bytes());
            }
            n => {
                f.push(0x80 | 127);
                f.extend_from_slice(&(n as u64).to_be_bytes());
            }
        }
        f.extend_from_slice(&mask);
        f.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        f
    }

    #[test]
    fn handshake_matches_the_rfc_example() {
        let raw = b"GET /api/ws HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: keep-alive, Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n";
        let req = crate::http::read_request(&mut &raw[..]).unwrap().unwrap();
        let resp = accept(&req).unwrap();
        assert_eq!(resp.status, 101);
        assert!(
            resp.headers
                .iter()
                .any(|(k, v)| k == "Sec-WebSocket-Accept" && v == "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=")
        );
        let plain = b"GET / HTTP/1.1\r\nHost: x\r\n\r\n";
        let req = crate::http::read_request(&mut &plain[..]).unwrap().unwrap();
        assert!(accept(&req).is_err());
    }

    #[test]
    fn messages_join_fragments_answer_pings_and_close() {
        let mut input = vec![];
        input.extend(client_frame(0x1, false, b"hel"));
        input.extend(client_frame(0x9, true, b"p"));
        input.extend(client_frame(0x0, true, "lo 桜".as_bytes()));
        let big = vec![b'x'; 70_000];
        input.extend(client_frame(0x2, true, &big));
        input.extend(client_frame(0x8, true, b""));
        let mut r = &input[..];
        let mut out = vec![];
        let mut pong = |f: Vec<u8>| out.extend(f);
        assert_eq!(
            read_message(&mut r, &mut pong).unwrap(),
            Message::Text("hello 桜".into())
        );
        assert_eq!(
            read_message(&mut r, &mut pong).unwrap(),
            Message::Binary(big)
        );
        assert_eq!(read_message(&mut r, &mut pong).unwrap(), Message::Close);
        // The ping was answered with a pong carrying its payload.
        assert_eq!(out, vec![0x8A, 1, b'p']);
    }

    #[test]
    fn bad_frames_are_errors() {
        let mut out = |_: Vec<u8>| {};
        let unmasked = [0x81u8, 0x01, b'a'];
        assert!(read_message(&mut &unmasked[..], &mut out).is_err());
        let stray = client_frame(0x0, true, b"x");
        assert!(read_message(&mut &stray[..], &mut out).is_err());
        let mut huge = vec![0x82u8, 0x80 | 127];
        huge.extend_from_slice(&(MAX_MESSAGE as u64 + 1).to_be_bytes());
        huge.extend_from_slice(&[0; 4]);
        assert!(read_message(&mut &huge[..], &mut out).is_err());
        let bad_utf8 = client_frame(0x1, true, &[0xff, 0xfe]);
        assert!(read_message(&mut &bad_utf8[..], &mut out).is_err());
    }

    #[test]
    fn a_frame_goes_out_in_one_write() {
        /// Counts the write calls a frame takes.
        struct Writes(Vec<Vec<u8>>);
        impl Write for Writes {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.push(b.to_vec());
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        for n in [5usize, 300, 70_000] {
            let mut w = Writes(vec![]);
            write_text(&mut w, &"a".repeat(n)).unwrap();
            assert_eq!(w.0.len(), 1, "a {n}-byte message took {} writes", w.0.len());
            assert_eq!(w.0[0], frame(0x1, "a".repeat(n).as_bytes()));
        }
    }

    #[test]
    fn server_frames_use_the_right_length_forms() {
        for (n, head) in [(5usize, 2usize), (300, 4), (70_000, 10)] {
            let mut out = vec![];
            write_text(&mut out, &"a".repeat(n)).unwrap();
            assert_eq!(out.len(), head + n);
            assert_eq!(out[0], 0x81);
        }
    }
}
