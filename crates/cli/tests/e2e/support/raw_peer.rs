// SPDX-License-Identifier: MPL-2.0
//! Blocking raw protocol peers built directly on `dropumi-protocol`.
//!
//! They speak the real protocol (or deliberately break it) to exercise the
//! receiver's validation. That this works with plain `std::net` also shows
//! the protocol crate is sans-IO.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use dropumi_domain::device::DeviceKind;
use dropumi_domain::peer::Intent;
use dropumi_protocol::frame::Frame;
use dropumi_protocol::handshake::{HandshakeOutcome, Initiator, Responder};
use dropumi_protocol::identity::{StaticSecret, fingerprint_of};
use dropumi_protocol::message::{Decoded, Hello, Message, Role, decode, encode};
use dropumi_protocol::record::{LEN_PREFIX, MAX_RECORD, Opener, Sealer, frame_handshake};
use dropumi_protocol::version::{Preamble, PreambleReply, Prologue};

/// A raw identity with its full fingerprint.
pub struct RawIdentity {
    pub secret: StaticSecret,
    pub fingerprint: String,
}

impl RawIdentity {
    /// A deterministic identity from a seed.
    #[must_use]
    pub fn new(seed: u8) -> Self {
        let secret = StaticSecret::from_bytes([seed; 32]);
        let fingerprint = fingerprint_of(&secret.public_key().unwrap()).to_full();
        Self {
            secret,
            fingerprint,
        }
    }
}

fn read_exact(s: &mut TcpStream, n: usize) -> std::io::Result<Vec<u8>> {
    let mut b = vec![0u8; n];
    s.read_exact(&mut b)?;
    Ok(b)
}

fn read_handshake(s: &mut TcpStream) -> std::io::Result<Vec<u8>> {
    let len = read_exact(s, 2)?;
    read_exact(s, usize::from(u16::from_be_bytes([len[0], len[1]])))
}

/// Sends a preamble and returns everything the responder sent until it
/// closed (or 5 s passed).
pub fn preamble_only(addr: SocketAddr, raw_preamble: &[u8]) -> Vec<u8> {
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s.write_all(raw_preamble).unwrap();
    let mut out = Vec::new();
    let _ = s.read_to_end(&mut out);
    out
}

/// Opens a TCP connection, sends `bytes`, and reports whether the peer
/// closed within 10 s.
pub fn send_raw_and_wait_close(addr: SocketAddr, bytes: &[u8]) -> bool {
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let _ = s.write_all(bytes);
    let mut sink = [0u8; 1024];
    loop {
        match s.read(&mut sink) {
            Ok(0) => return true,
            Ok(_) => {}
            Err(e) => {
                return !matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                );
            }
        }
    }
}

/// An authenticated raw session (initiator side).
pub struct RawSession {
    pub stream: TcpStream,
    sealer: Sealer,
    opener: Opener,
    out: Vec<u8>,
    plain: Vec<u8>,
}

impl RawSession {
    /// Connects, negotiates v1, runs the handshake and exchanges Hellos.
    pub fn connect(
        addr: SocketAddr,
        id: &RawIdentity,
        intent: Intent,
    ) -> Result<(Self, Hello), String> {
        let mut s = TcpStream::connect(addr).map_err(|e| e.to_string())?;
        s.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
        s.set_nodelay(true).unwrap();
        let preamble = Preamble::new(&[1]).unwrap().encode();
        s.write_all(&preamble).map_err(|e| e.to_string())?;
        let reply = read_exact(&mut s, 8).map_err(|e| e.to_string())?;
        let prologue = Prologue::new(&preamble, &reply).unwrap();
        let (st, m1) = Initiator::start(&id.secret, &prologue, [7; 32]).unwrap();
        s.write_all(&frame_handshake(&m1).unwrap())
            .map_err(|e| e.to_string())?;
        let m2 = read_handshake(&mut s).map_err(|e| e.to_string())?;
        let (done, m3) = st
            .read_msg2(&m2)
            .map_err(|e| e.to_string())?
            .proceed()
            .unwrap();
        s.write_all(&frame_handshake(&m3).unwrap())
            .map_err(|e| e.to_string())?;
        let HandshakeOutcome { sealer, opener, .. } = done.finish();
        let mut session = Self {
            stream: s,
            sealer,
            opener,
            out: vec![0u8; MAX_RECORD],
            plain: vec![0u8; MAX_RECORD],
        };
        session.send(&Message::Hello(Hello {
            device_name: "Raw peer".into(),
            device_kind: DeviceKind::Unknown,
            software: Some("raw-test".into()),
            features: vec![],
            hash_algs: vec![1],
            intent: Some(intent),
            sas_reveal: None,
        }));
        match session.recv() {
            Some(Message::Hello(h)) => Ok((session, h)),
            Some(other) => Err(format!("expected Hello, got {other:?}")),
            None => Err("closed before Hello".into()),
        }
    }

    /// Seals and writes an arbitrary plaintext (frame type included).
    pub fn send_plain(&mut self, plain: &[u8]) {
        let n = self.sealer.seal(plain, &mut self.out).unwrap();
        let _ = self.stream.write_all(&self.out[..n]);
    }

    /// Sends raw CBOR as a CONTROL frame.
    pub fn send_control_bytes(&mut self, cbor: &[u8]) {
        let mut p = vec![1u8];
        p.extend_from_slice(cbor);
        self.send_plain(&p);
    }

    /// Sends a message.
    pub fn send(&mut self, msg: &Message) {
        let mut cbor = Vec::new();
        encode(msg, &mut cbor).unwrap();
        self.send_control_bytes(&cbor);
    }

    /// Sends file data.
    pub fn send_data(&mut self, data: &[u8]) {
        let mut out = Vec::new();
        Frame::Data(data).encode_into(&mut out).unwrap();
        self.send_plain(&out);
    }

    /// Writes bytes that are not a valid record.
    pub fn send_garbage_record(&mut self, len: u16) {
        let mut b = len.to_be_bytes().to_vec();
        b.extend(std::iter::repeat_n(0x5Au8, usize::from(len)));
        let _ = self.stream.write_all(&b);
    }

    /// Next control message, or `None` on close, error or timeout.
    pub fn recv(&mut self) -> Option<Message> {
        let mut len = [0u8; LEN_PREFIX];
        self.stream.read_exact(&mut len).ok()?;
        let n = usize::from(u16::from_be_bytes(len));
        let mut ct = vec![0u8; n];
        self.stream.read_exact(&mut ct).ok()?;
        let plain = self.opener.open(&ct, &mut self.plain).ok()?;
        match Frame::decode(plain).ok()? {
            Frame::Control(body) => match decode(body, Role::Responder).ok()? {
                Decoded::Message(m) => Some(m),
                Decoded::Ignored(_) => self.recv(),
            },
            Frame::Data(_) => None,
        }
    }

    /// Reads until the receiver closes; returns the messages received and
    /// whether it closed within the timeout.
    pub fn until_closed(&mut self) -> (Vec<Message>, bool) {
        let mut msgs = Vec::new();
        loop {
            let mut len = [0u8; LEN_PREFIX];
            match self.stream.read_exact(&mut len) {
                Ok(()) => {}
                Err(e) => {
                    let closed = !matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    );
                    return (msgs, closed);
                }
            }
            let n = usize::from(u16::from_be_bytes(len));
            let mut ct = vec![0u8; n];
            if self.stream.read_exact(&mut ct).is_err() {
                return (msgs, true);
            }
            if let Ok(plain) = self.opener.open(&ct, &mut self.plain)
                && let Ok(Frame::Control(body)) = Frame::decode(plain)
                && let Ok(Decoded::Message(m)) = decode(body, Role::Responder)
            {
                msgs.push(m);
            }
        }
    }
}

/// A raw responder that answers every preamble with NO_COMMON_VERSION [2].
pub fn no_common_version_responder() -> (SocketAddr, std::thread::JoinHandle<()>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let h = std::thread::spawn(move || {
        if let Ok((mut s, _)) = l.accept() {
            let _ = read_exact(&mut s, 8);
            let _ = s.write_all(&PreambleReply::NoCommonVersion(vec![2]).encode());
        }
    });
    (addr, h)
}

/// A man-in-the-middle responder with its own key. Reports whether the
/// initiator ever sent message 3 (it must not, if it pinned the real peer).
pub fn impostor_responder(id: RawIdentity) -> (SocketAddr, std::thread::JoinHandle<bool>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    let h = std::thread::spawn(move || {
        let Ok((mut s, _)) = l.accept() else {
            return false;
        };
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let Ok(preamble) = read_exact(&mut s, 8) else {
            return false;
        };
        let reply = PreambleReply::Ok(1).encode();
        let _ = s.write_all(&reply);
        let prologue = Prologue::new(&preamble, &reply).unwrap();
        let st = Responder::start(&id.secret, &prologue, [9; 32]).unwrap();
        let Ok(m1) = read_handshake(&mut s) else {
            return false;
        };
        let Ok((_st3, m2)) = st.read_msg1(&m1) else {
            return false;
        };
        let _ = s.write_all(&frame_handshake(&m2).unwrap());
        read_handshake(&mut s).is_ok()
    });
    (addr, h)
}
