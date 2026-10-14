// SPDX-License-Identifier: MPL-2.0
//! The app facade (`dropumi-ffi`, in this test process) against a real
//! `dropumi` CLI process on loopback: both implementations of the same core
//! must interoperate through the real protocol.

use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use dropumi_ffi::{
    AcceptMode, CallbackError, Direction, DropumiNode, EventListener, ForeignFile, ForeignReader,
    NodeConfig, NodeEvent, PairingResult, Prompt, ReceiveOptions, ReceivePolicy, SendItem,
    SendOptions, SendTarget, TransferResult, TransferStatus,
};

use crate::support::*;

struct Recorder(Mutex<Sender<NodeEvent>>);

impl EventListener for Recorder {
    fn on_events(&self, events: Vec<NodeEvent>) -> Result<(), CallbackError> {
        let tx = self.0.lock().unwrap();
        for e in events {
            let _ = tx.send(e);
        }
        Ok(())
    }
}

/// A node in temporary directories.
struct App {
    node: Arc<DropumiNode>,
    rx: Receiver<NodeEvent>,
    inbox: std::path::PathBuf,
    fingerprint: String,
    _dir: tempfile::TempDir,
}

impl App {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let inbox = dir.path().join("inbox");
        std::fs::create_dir(&inbox).unwrap();
        let (tx, rx) = channel();
        let node = DropumiNode::new(
            NodeConfig {
                config_dir: dir.path().join("config").to_string_lossy().into_owned(),
                receive_dir: Some(inbox.to_string_lossy().into_owned()),
            },
            Arc::new(Recorder(Mutex::new(tx))),
            None,
        )
        .unwrap();
        let fingerprint = node.identity().unwrap().fingerprint;
        Self {
            node,
            rx,
            inbox,
            fingerprint,
            _dir: dir,
        }
    }

    fn wait(&self, what: &str, pred: impl Fn(&NodeEvent) -> bool) -> NodeEvent {
        let deadline = Instant::now() + STEP;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let e = self
                .rx
                .recv_timeout(left)
                .unwrap_or_else(|_| panic!("app: timed out waiting for {what}"));
            if pred(&e) {
                return e;
            }
        }
    }

    fn prompt(&self) -> Prompt {
        match self.wait("a prompt", |e| matches!(e, NodeEvent::PromptOpened { .. })) {
            NodeEvent::PromptOpened { prompt } => prompt,
            _ => unreachable!(),
        }
    }

    fn finished(&self, direction: Direction) -> TransferResult {
        match self.wait(
            "a finished transfer",
            |e| matches!(e, NodeEvent::TransferFinished { direction: d, .. } if *d == direction),
        ) {
            NodeEvent::TransferFinished { result, .. } => result,
            _ => unreachable!(),
        }
    }

    fn send_path(&self, port: u16, file: &Path, allow_unverified: bool) {
        self.node
            .send(
                to(port),
                vec![SendItem::Path {
                    path: file.to_string_lossy().into_owned(),
                }],
                SendOptions { allow_unverified },
            )
            .unwrap();
    }

    fn receive(&self, policy: ReceivePolicy, accept: AcceptMode) -> u16 {
        self.node
            .start_receiving(ReceiveOptions {
                policy,
                accept,
                port: Some(0),
                advertise: false,
            })
            .unwrap()
            .port
    }
}

fn to(port: u16) -> SendTarget {
    SendTarget::Address {
        address: format!("127.0.0.1:{port}"),
        expect_fingerprint: None,
    }
}

#[test]
fn ffi_sends_to_cli_trusted() {
    let app = App::new();
    let cli = Device::new("Cli");
    cli.trust_fp(&app.fingerprint, "App");
    app.node
        .trust_fingerprint(cli.fingerprint.clone(), Some("Cli".into()))
        .unwrap();
    let dst = tempfile::tempdir().unwrap();
    let (mut rx, port) = receiver(&cli, dst.path(), &["--accept", "trusted", "--once"]);
    let src = tempfile::tempdir().unwrap();
    let f = write_pattern(&src.path().join("from app.bin"), 3 * 1024 * 1024 + 7, 11);
    app.send_path(port, &f, false);
    assert_eq!(
        app.finished(Direction::Outgoing).status,
        TransferStatus::Completed
    );
    assert_eq!(rx.wait_exit(EXIT), 0, "{}", rx.stderr());
    assert_eq!(sha256(&f), sha256(&dst.path().join("from app.bin")));
}

#[test]
fn cli_sends_to_ffi_prompt_accept() {
    let app = App::new();
    let cli = Device::new("Cli");
    cli.trust_fp(&app.fingerprint, "App");
    app.node
        .trust_fingerprint(cli.fingerprint.clone(), Some("Cli".into()))
        .unwrap();
    let port = app.receive(ReceivePolicy::TrustedOnly, AcceptMode::Prompt);
    let src = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("from cli.txt"), b"hello app");
    let mut tx = sender(&cli, std::slice::from_ref(&f), port, &[]);
    let Prompt::Offer { id, peer, code, .. } = app.prompt() else {
        panic!("expected an offer");
    };
    assert!(peer.trusted);
    assert_eq!(peer.alias.as_deref(), Some("Cli"));
    assert_eq!(code, None);
    app.node.answer_offer(id, true).unwrap();
    assert_eq!(tx.wait_exit(EXIT), 0, "{}", tx.stderr());
    assert_eq!(
        app.finished(Direction::Incoming).status,
        TransferStatus::Completed
    );
    assert_eq!(sha256(&f), sha256(&app.inbox.join("from cli.txt")));
}

#[test]
fn ffi_cli_pairing_sas_matches() {
    let app = App::new();
    let cli = Device::new("Cli");
    let mut l = Proc::spawn(
        "pair-listen",
        cli.config(),
        &args(&[
            "pair",
            "--listen",
            "--bind",
            "127.0.0.1",
            "--port",
            "0",
            "--no-mdns",
        ]),
    );
    let port = u16::try_from(l.wait_event("listening")["port"].as_u64().unwrap()).unwrap();
    app.node.pair_with(to(port)).unwrap();
    let Prompt::Code { id, code, .. } = app.prompt() else {
        panic!("expected a code prompt");
    };
    let cli_code = l.wait_event("sas")["code"].as_str().unwrap().to_owned();
    assert_eq!(code, cli_code);
    app.node.answer_sas(id, true).unwrap();
    l.cmd("confirm");
    assert_eq!(l.wait_exit(EXIT), 0, "{}", l.stderr());
    assert!(matches!(
        app.wait("pairing to end", |e| matches!(
            e,
            NodeEvent::PairingFinished { .. }
        )),
        NodeEvent::PairingFinished {
            result: PairingResult::Paired { .. },
            ..
        }
    ));
    assert_eq!(cli.trusted(), vec![app.fingerprint.clone()]);
    assert_eq!(
        app.node.trusted_devices().unwrap()[0].fingerprint,
        cli.fingerprint
    );
}

/// A document held after its first 100,000 bytes until released.
struct Held {
    data: Vec<u8>,
    released: Arc<(Mutex<bool>, std::sync::Condvar)>,
}

struct HeldReader {
    data: Vec<u8>,
    pos: Mutex<usize>,
    released: Arc<(Mutex<bool>, std::sync::Condvar)>,
}

impl ForeignFile for Held {
    fn display_name(&self) -> String {
        "held.bin".into()
    }
    fn size(&self) -> Option<u64> {
        Some(self.data.len() as u64)
    }
    fn modified(&self) -> Option<i64> {
        None
    }
    fn open(&self) -> Result<Arc<dyn ForeignReader>, CallbackError> {
        Ok(Arc::new(HeldReader {
            data: self.data.clone(),
            pos: Mutex::new(0),
            released: Arc::clone(&self.released),
        }))
    }
}

impl ForeignReader for HeldReader {
    fn read(&self, max_len: u32) -> Result<Vec<u8>, CallbackError> {
        let mut pos = self.pos.lock().unwrap();
        if *pos >= 100_000 {
            let (lock, cv) = &*self.released;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = cv.wait(released).unwrap();
            }
        }
        let limit = if *pos < 100_000 {
            100_000
        } else {
            self.data.len()
        };
        let end = (*pos + max_len as usize).min(limit);
        let chunk = self.data[*pos..end].to_vec();
        *pos = end;
        Ok(chunk)
    }
    fn close(&self) {}
}

#[test]
fn ffi_cancel_seen_by_cli() {
    let app = App::new();
    let cli = Device::new("Cli");
    cli.trust_fp(&app.fingerprint, "App");
    app.node
        .trust_fingerprint(cli.fingerprint.clone(), None)
        .unwrap();
    let dst = tempfile::tempdir().unwrap();
    let (mut rx, port) = receiver(&cli, dst.path(), &["--accept", "trusted", "--once"]);
    let released = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    let file: Arc<dyn ForeignFile> = Arc::new(Held {
        data: vec![7u8; 400_000],
        released: Arc::clone(&released),
    });
    let op = app
        .node
        .send(
            to(port),
            vec![SendItem::Stream { file }],
            SendOptions {
                allow_unverified: false,
            },
        )
        .unwrap();
    rx.wait_event("accepted");
    op.cancel();
    let r = app.finished(Direction::Outgoing);
    assert_eq!(r.status, TransferStatus::Cancelled);
    assert_eq!(rx.wait_exit(EXIT), 4, "{}", rx.stderr());
    assert!(rx.saw("cancelled"));
    let (lock, cv) = &*released;
    *lock.lock().unwrap() = true;
    cv.notify_all();
    assert!(std::fs::read_dir(dst.path()).unwrap().next().is_none());
}

#[test]
fn cli_everyone_offer_to_ffi_shows_sas() {
    let app = App::new();
    let cli = Device::new("Cli");
    let port = app.receive(
        ReceivePolicy::Everyone { minutes: 10 },
        AcceptMode::AutoTrusted,
    );
    let src = tempfile::tempdir().unwrap();
    let f = write(
        &src.path().join("unverified.txt"),
        b"compare the code first",
    );
    let mut tx = sender(
        &cli,
        std::slice::from_ref(&f),
        port,
        &["--allow-unverified"],
    );
    // The CLI shows its code and waits for confirmation.
    let cli_code = tx.wait_event("sas")["code"].as_str().unwrap().to_owned();
    tx.cmd("confirm");
    let Prompt::Offer {
        id,
        code: Some(code),
        peer,
        ..
    } = app.prompt()
    else {
        panic!("expected an offer with a code");
    };
    assert!(!peer.trusted);
    assert_eq!(code, cli_code);
    app.node.answer_offer(id, true).unwrap();
    assert_eq!(tx.wait_exit(EXIT), 0, "{}", tx.stderr());
    assert_eq!(sha256(&f), sha256(&app.inbox.join("unverified.txt")));
    assert!(app.node.trusted_devices().unwrap().is_empty());
}
