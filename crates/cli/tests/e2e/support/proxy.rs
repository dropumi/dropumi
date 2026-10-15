// SPDX-License-Identifier: MPL-2.0
//! A transparent TCP proxy that captures traffic and can tamper with it.

use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// What to do to the client-to-server stream.
#[derive(Debug, Clone)]
pub enum Tamper {
    /// Nothing.
    None,
    /// Replace an exact prefix with other bytes.
    RewritePrefix(Vec<u8>, Vec<u8>),
    /// Flip the low bit of the byte at this offset.
    FlipAt(usize),
}

/// A running proxy for one connection.
pub struct Proxy {
    pub addr: SocketAddr,
    pub client_to_server: Arc<Mutex<Vec<u8>>>,
    pub server_to_client: Arc<Mutex<Vec<u8>>>,
    handle: Option<JoinHandle<()>>,
}

fn pump(mut from: TcpStream, mut to: TcpStream, capture: Arc<Mutex<Vec<u8>>>, tamper: Tamper) {
    let mut buf = [0u8; 16 * 1024];
    let mut offset = 0usize;
    let mut pending_prefix: Option<(Vec<u8>, Vec<u8>)> = match tamper.clone() {
        Tamper::RewritePrefix(a, b) => Some((a, b)),
        _ => None,
    };
    let mut prefix_buf = Vec::new();
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let mut chunk = buf[..n].to_vec();
        capture.lock().unwrap().extend_from_slice(&chunk);
        if let Some((old, new)) = pending_prefix.as_ref() {
            prefix_buf.extend_from_slice(&chunk);
            if prefix_buf.len() < old.len() {
                continue;
            }
            if prefix_buf.starts_with(old) {
                let mut rewritten = new.clone();
                rewritten.extend_from_slice(&prefix_buf[old.len()..]);
                chunk = rewritten;
            } else {
                chunk = std::mem::take(&mut prefix_buf);
            }
            pending_prefix = None;
        }
        if let Tamper::FlipAt(at) = tamper
            && at >= offset
            && at < offset + chunk.len()
        {
            chunk[at - offset] ^= 0x01;
        }
        offset += n;
        if to.write_all(&chunk).is_err() {
            break;
        }
    }
    let _ = to.shutdown(Shutdown::Write);
}

impl Proxy {
    /// Proxies one connection to `target`.
    #[must_use]
    pub fn start(target: SocketAddr, tamper: Tamper) -> Self {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let c2s = Arc::new(Mutex::new(Vec::new()));
        let s2c = Arc::new(Mutex::new(Vec::new()));
        let (c2s2, s2c2) = (Arc::clone(&c2s), Arc::clone(&s2c));
        let handle = std::thread::spawn(move || {
            let Ok((client, _)) = l.accept() else { return };
            let Ok(server) = TcpStream::connect(target) else {
                return;
            };
            let (c_r, c_w) = (client.try_clone().unwrap(), client);
            let (s_r, s_w) = (server.try_clone().unwrap(), server);
            let up = std::thread::spawn(move || pump(c_r, s_w, c2s2, tamper));
            let down = std::thread::spawn(move || pump(s_r, c_w, s2c2, Tamper::None));
            let _ = up.join();
            let _ = down.join();
        });
        Self {
            addr,
            client_to_server: c2s,
            server_to_client: s2c,
            handle: Some(handle),
        }
    }

    /// Waits for the proxied connection to finish.
    pub fn join(&mut self) {
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }

    /// Everything captured in both directions.
    #[must_use]
    pub fn captured(&self) -> Vec<u8> {
        let mut all = self.client_to_server.lock().unwrap().clone();
        all.extend_from_slice(&self.server_to_client.lock().unwrap());
        all
    }
}

/// Whether `needle` occurs in `haystack`.
#[must_use]
pub fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}
