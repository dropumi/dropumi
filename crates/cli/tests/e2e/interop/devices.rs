// SPDX-License-Identifier: MPL-2.0
//! Device to device: two real apps (each its real core, inside its interop
//! test in scenario `agent`) pair with each other and send files both ways
//! over Wi-Fi, steered step by step by this host.
//!
//! Opt-in and macOS only:
//!
//! ```sh
//! bash scripts/devices/interop.sh      # builds and installs, then runs these
//! ```
//!
//! which runs `DROPUMI_TEST_DEVICES=1 cargo test -p dropumi-cli --test e2e
//! --locked -- --ignored --test-threads=1 devices_`. Each test also needs
//! the variables of its devices: `DROPUMI_TEST_APPLE_DEVICE` (the iPhone's
//! UDID) and `DROPUMI_TEST_LAN_ADDR` (this Mac's Wi-Fi address, which the
//! iPhone dials) for the iPhone, `ANDROID_SERIAL` for the Android phone.
//!
//! The control protocol (test only; the apps' production code is not
//! involved): this host listens, one listener per agent, and passes its
//! address at launch (`TEST_RUNNER_DROPUMI_CONTROL` for the Apple XCTest,
//! the runner argument `dropumi_control` for Android, through `adb reverse`).
//! The agent connects and sends `{"event":"hello",…}`; then this host sends
//! one command at a time (`{"id":n,"cmd":…}`) and reads its reply (same
//! `id`, `"ok":true` or `"ok":false,"error":…`). Unsolicited events (no
//! `id`, an `"event"` field) are queued per agent until a step waits for
//! them. Every line is one JSON object. Waits block on the socket with a
//! deadline; nothing sleeps.
//!
//! Each agent confirms a code or accepts an offer only if the authenticated
//! fingerprint is the one this host pinned (`expect_fp`). This host then
//! checks that both sides showed the same code, that each side trusts the
//! other afterwards, that a paired send shows no code, and that every
//! received SHA-256 matches the sent one and the pattern's own.
//!
//! The device-to-device traffic uses the Wi-Fi addresses the agents report;
//! only Android's control channel goes through adb. Control traffic and
//! these logs carry IP addresses: keep them in the test output.

use std::collections::{BTreeMap, VecDeque};
use std::io::{ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{android, apple};

/// Installing and launching on the iPhone takes longer than elsewhere.
const LAUNCH_IPHONE: Duration = Duration::from_secs(180);
const LAUNCH: Duration = Duration::from_secs(90);
/// Each step: a reply or an awaited event.
const STEP: Duration = Duration::from_secs(60);
/// A longer control line is a broken agent.
const MAX_LINE: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug)]
enum Kind {
    /// The iPhone named by `DROPUMI_TEST_APPLE_DEVICE`.
    Iphone,
    /// The Mac app on this host.
    Mac,
    /// The Android phone named by `ANDROID_SERIAL`.
    Android,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Self::Iphone => "iphone",
            Self::Mac => "mac",
            Self::Android => "android",
        }
    }

    /// The `platform` of the agent's hello.
    fn platform(self) -> &'static str {
        match self {
            Self::Iphone => "ios",
            Self::Mac => "macos",
            Self::Android => "android",
        }
    }

    fn launch_timeout(self) -> Duration {
        match self {
            Self::Iphone => LAUNCH_IPHONE,
            Self::Mac | Self::Android => LAUNCH,
        }
    }

    /// The address the agent dials and this host's control listener binds:
    /// loopback for the Mac app and for Android (through `adb reverse`),
    /// this Mac's Wi-Fi address for the iPhone.
    fn control_host(self) -> String {
        match self {
            Self::Mac | Self::Android => "127.0.0.1".into(),
            Self::Iphone => lan_addr().expect("set DROPUMI_TEST_LAN_ADDR"),
        }
    }

    /// The variables this kind of device needs, besides `DROPUMI_TEST_DEVICES`.
    fn required_env(self) -> &'static [&'static str] {
        match self {
            Self::Iphone => &["DROPUMI_TEST_APPLE_DEVICE", "DROPUMI_TEST_LAN_ADDR"],
            Self::Mac => &[],
            Self::Android => &["ANDROID_SERIAL"],
        }
    }
}

fn env_set(var: &str) -> bool {
    std::env::var(var).is_ok_and(|v| !v.is_empty())
}

fn lan_addr() -> Option<String> {
    std::env::var("DROPUMI_TEST_LAN_ADDR")
        .ok()
        .filter(|v| !v.is_empty())
}

/// Whether the pair may run; prints why not.
fn enabled(kinds: &[Kind]) -> bool {
    let mut missing = Vec::new();
    if std::env::var("DROPUMI_TEST_DEVICES").as_deref() != Ok("1") {
        missing.push("DROPUMI_TEST_DEVICES=1");
    }
    for var in kinds.iter().flat_map(|k| k.required_env()) {
        if !env_set(var) && !missing.contains(var) {
            missing.push(var);
        }
    }
    if missing.is_empty() {
        return true;
    }
    eprintln!(
        "skipped: set {} (see scripts/devices/interop.sh)",
        missing.join(", ")
    );
    false
}

/// The launched interop test that hosts an agent.
enum App {
    Apple(apple::AppTest),
    Android {
        test: android::AppTest,
        /// Dropped after the test: the control channel's `adb reverse`.
        _forwards: android::Forwards,
    },
}

impl App {
    fn tail(&self) -> String {
        match self {
            Self::Apple(test) => test.tail(),
            Self::Android { test, .. } => test.tail(),
        }
    }

    /// Waits for the test process to end; its own assertions must pass.
    fn finish(self) {
        match self {
            Self::Apple(test) => test.finish(),
            Self::Android { test, _forwards } => test.finish(),
        }
    }
}

/// One agent's control connection: line-delimited JSON.
struct Control {
    stream: TcpStream,
    pending: Vec<u8>,
    label: &'static str,
}

impl Control {
    fn send(&mut self, message: &Value) {
        let text = message.to_string();
        eprintln!("[{}] -> {text}", self.label);
        let mut line = text.into_bytes();
        line.push(b'\n');
        self.stream
            .write_all(&line)
            .and_then(|()| self.stream.flush())
            .unwrap_or_else(|e| panic!("[{}] control write failed: {e}", self.label));
    }

    /// The next message before `deadline`, blocking on the socket.
    fn next(&mut self, deadline: Instant) -> Result<Value, String> {
        loop {
            if let Some(end) = self.pending.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.pending.drain(..=end).collect();
                let text = std::str::from_utf8(&line[..end])
                    .map_err(|_| "a control line is not UTF-8".to_owned())?
                    .trim_end_matches('\r');
                if text.trim().is_empty() {
                    continue;
                }
                eprintln!("[{}] <- {text}", self.label);
                let value: Value = serde_json::from_str(text)
                    .map_err(|e| format!("a control line is not JSON ({e}): {text}"))?;
                if !value.is_object() {
                    return Err(format!("a control line is not an object: {text}"));
                }
                return Ok(value);
            }
            if self.pending.len() > MAX_LINE {
                return Err("a control line is too long".into());
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err("timed out".into());
            }
            self.stream
                .set_read_timeout(Some(left))
                .map_err(|e| e.to_string())?;
            let mut chunk = [0u8; 16 * 1024];
            match self.stream.read(&mut chunk) {
                Ok(0) => return Err("the agent closed the control connection".into()),
                Ok(n) => self.pending.extend_from_slice(&chunk[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(format!("control read failed: {e}")),
            }
        }
    }
}

/// An agent whose test was launched but that has not connected yet.
struct Starting {
    kind: Kind,
    app: App,
    accepted: Receiver<std::io::Result<(TcpStream, SocketAddr)>>,
    deadline: Instant,
}

impl Starting {
    /// Opens the control listener and launches the agent's test, which
    /// dials it.
    fn launch(kind: Kind) -> Self {
        let host = kind.control_host();
        let listener = TcpListener::bind((host.as_str(), 0))
            .unwrap_or_else(|e| panic!("bind the control listener on {host}: {e}"));
        let port = listener.local_addr().unwrap().port();
        let control = format!("{host}:{port}");
        let deadline = Instant::now() + kind.launch_timeout();
        let (tx, accepted) = channel();
        // One connection: the agent's. A thread, so the wait has a deadline.
        std::thread::spawn(move || {
            let _ = tx.send(listener.accept());
        });
        let app = match kind {
            Kind::Mac => App::Apple(apple::AppTest::start(
                apple::Platform::MacOs,
                "agent",
                &[("DROPUMI_CONTROL", &control)],
            )),
            Kind::Iphone => App::Apple(apple::AppTest::start(
                apple::Platform::IosDevice,
                "agent",
                &[("DROPUMI_CONTROL", &control)],
            )),
            Kind::Android => {
                let mut forwards = android::Forwards::default();
                forwards.reverse_to_cli(port);
                App::Android {
                    test: android::AppTest::start("agent", &[("dropumi_control", &control)]),
                    _forwards: forwards,
                }
            }
        };
        Self {
            kind,
            app,
            accepted,
            deadline,
        }
    }

    /// Waits for the agent's connection and its hello.
    fn connect(self) -> Agent {
        let label = self.kind.label();
        let left = self.deadline.saturating_duration_since(Instant::now());
        let stream = match self.accepted.recv_timeout(left) {
            Ok(Ok((stream, _))) => stream,
            Ok(Err(e)) => panic!("[{label}] control accept failed: {e}"),
            Err(_) => panic!(
                "[{label}] the agent did not connect in {:?}\n{}",
                self.kind.launch_timeout(),
                self.app.tail()
            ),
        };
        stream.set_nodelay(true).unwrap();
        let mut agent = Agent {
            kind: self.kind,
            fp: String::new(),
            hello: Value::Null,
            control: Control {
                stream,
                pending: Vec::new(),
                label,
            },
            events: VecDeque::new(),
            next_id: 0,
            app: self.app,
        };
        let hello = agent.read(self.deadline);
        agent.check(
            hello.get("id").is_none() && hello["event"] == "hello",
            &format!("the first line is not a hello: {hello}"),
        );
        agent.check(
            hello["platform"] == self.kind.platform(),
            &format!("expected platform {}: {hello}", self.kind.platform()),
        );
        let fp = hello["fingerprint"].as_str().unwrap_or("").to_owned();
        agent.check(fp.len() == 52, &format!("not a full fingerprint: {hello}"));
        agent.fp = fp;
        agent.hello = hello;
        agent
    }
}

/// A connected agent.
struct Agent {
    kind: Kind,
    fp: String,
    hello: Value,
    control: Control,
    /// Events read while waiting for something else.
    events: VecDeque<Value>,
    next_id: u64,
    app: App,
}

impl Agent {
    fn fail(&self, message: &str) -> ! {
        panic!(
            "[{}] {message}\n--- test output ---\n{}",
            self.kind.label(),
            self.app.tail()
        )
    }

    fn check(&self, ok: bool, message: &str) {
        if !ok {
            self.fail(message);
        }
    }

    fn read(&mut self, deadline: Instant) -> Value {
        match self.control.next(deadline) {
            Ok(v) => v,
            Err(e) => self.fail(&e),
        }
    }

    /// Sends one command and waits for its reply, which must be `ok`.
    fn request(&mut self, cmd: &str, fields: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let mut message = fields;
        message["id"] = json!(id);
        message["cmd"] = json!(cmd);
        self.control.send(&message);
        let deadline = Instant::now() + STEP;
        loop {
            let v = self.read(deadline);
            if v.get("id").is_some() {
                self.check(
                    v["id"].as_u64() == Some(id),
                    &format!("a reply to another command (waiting for {id}): {v}"),
                );
                self.check(
                    v["ok"] == true,
                    &format!("{cmd} failed: {}", v["error"].as_str().unwrap_or("")),
                );
                return v;
            }
            self.check(v.get("event").is_some(), &format!("not an event: {v}"));
            self.events.push_back(v);
        }
    }

    /// The first `event` (queued or still to come) within one step.
    fn wait_event(&mut self, event: &str) -> Value {
        if let Some(at) = self.events.iter().position(|v| v["event"] == event) {
            return self.events.remove(at).unwrap();
        }
        let deadline = Instant::now() + STEP;
        loop {
            let v = self.read(deadline);
            self.check(
                v.get("id").is_none() && v.get("event").is_some(),
                &format!("waiting for {event}, got a stray message: {v}"),
            );
            if v["event"] == event {
                return v;
            }
            self.events.push_back(v);
        }
    }

    /// Whether an `event` was queued (and not taken by a step).
    fn saw(&self, event: &str) -> bool {
        self.events.iter().any(|v| v["event"] == event)
    }

    /// The peer address for device-to-device traffic: a Wi-Fi IPv4 address
    /// from a `receive` or `wait_pairing` reply, else from the hello (this
    /// Mac's `DROPUMI_TEST_LAN_ADDR` first, if listed), and the reply's port.
    fn address(&self, reply: &Value) -> String {
        let port = reply["port"].as_u64().and_then(|p| u16::try_from(p).ok());
        let Some(port) = port.filter(|p| *p != 0) else {
            self.fail(&format!("no port in the reply: {reply}"))
        };
        let mut candidates: Vec<Ipv4Addr> = [&reply["addresses"], &self.hello["addresses"]]
            .iter()
            .filter_map(|list| list.as_array())
            .flatten()
            .filter_map(|a| a.as_str()?.parse::<Ipv4Addr>().ok())
            .filter(|a| !a.is_loopback() && !a.is_unspecified())
            .collect();
        if let Some(lan) = lan_addr().and_then(|a| a.parse::<Ipv4Addr>().ok())
            && let Some(at) = candidates.iter().position(|a| *a == lan)
        {
            candidates.swap(0, at);
        }
        let Some(ip) = candidates.first() else {
            self.fail(&format!(
                "the agent reported no Wi-Fi IPv4 address: {reply}"
            ))
        };
        format!("{ip}:{port}")
    }

    fn trusted(&mut self) -> Vec<String> {
        let reply = self.request("trusted", json!({}));
        let Some(list) = reply["fingerprints"].as_array() else {
            self.fail(&format!("no fingerprints in the reply: {reply}"))
        };
        list.iter()
            .filter_map(|f| f.as_str().map(str::to_owned))
            .collect()
    }

    /// Ends the agent with `quit`; its test must then finish OK.
    fn quit(&mut self) {
        self.request("quit", json!({}));
    }

    fn finish(self) {
        let Self { control, app, .. } = self;
        drop(control);
        app.finish();
    }
}

/// The SHA-256 of the agents' file pattern: byte `i` is `(i * 31 + 7) % 251`.
fn pattern_sha256(len: u64) -> String {
    let mut h = Sha256::new();
    let mut block = Vec::with_capacity(64 * 1024);
    let mut i = 0u64;
    while i < len {
        block.clear();
        while i < len && block.len() < 64 * 1024 {
            block.push(((i * 31 + 7) % 251) as u8);
            i += 1;
        }
        h.update(&block);
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn is_code(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|c| c.len() == 6 && c.bytes().all(|b| b.is_ascii_digit()))
}

/// Step 2: B waits on its pairing window, A pairs with it; both show the
/// same code for the other's key, and each trusts the other afterwards.
fn pair(a: &mut Agent, b: &mut Agent) {
    assert!(
        !a.trusted().contains(&b.fp) && !b.trusted().contains(&a.fp),
        "fresh identities: nothing is paired before the pairing"
    );
    let window = b.request("wait_pairing", json!({ "expect_fp": a.fp }));
    let address = b.address(&window);
    a.request("pair", json!({ "address": address, "expect_fp": b.fp }));
    let code_a = a.wait_event("code");
    let code_b = b.wait_event("code");
    assert!(is_code(&code_a["code"]), "a 6-digit code: {code_a}");
    assert_eq!(
        code_a["code"], code_b["code"],
        "both sides show the same code"
    );
    assert_eq!(code_a["peer_fp"], b.fp.as_str(), "A saw B's key");
    assert_eq!(code_b["peer_fp"], a.fp.as_str(), "B saw A's key");
    let (fp_a, fp_b) = (a.fp.clone(), b.fp.clone());
    for (agent, peer_fp) in [(&mut *a, fp_b), (&mut *b, fp_a)] {
        let result = agent.wait_event("pair_result");
        assert_eq!(
            result["result"],
            "paired",
            "{} paired: {result}",
            agent.kind.label()
        );
        assert_eq!(result["peer_fp"], peer_fp.as_str());
    }
    let (trusted_a, trusted_b) = (a.trusted(), b.trusted());
    assert!(trusted_a.contains(&b.fp), "A trusts B: {trusted_a:?}");
    assert!(trusted_b.contains(&a.fp), "B trusts A: {trusted_b:?}");
}

/// Steps 3 and 4: `to` receives (paired only) and `from` sends `files` to
/// it; no code is shown and every hash matches.
fn send_paired(from: &mut Agent, to: &mut Agent, files: &[(&str, u64)]) {
    let listening = to.request(
        "receive",
        json!({ "policy": "paired", "expect_fp": from.fp }),
    );
    let address = to.address(&listening);
    let list: Vec<Value> = files
        .iter()
        .map(|(name, bytes)| json!({ "name": name, "bytes": bytes }))
        .collect();
    from.request(
        "send",
        json!({ "address": address, "expect_fp": to.fp, "files": list }),
    );
    let mut sent = BTreeMap::new();
    for _ in files {
        let e = from.wait_event("sent_file");
        let name = e["name"].as_str().unwrap_or("").to_owned();
        let entry = (
            e["bytes"].as_u64(),
            e["sha256"].as_str().map(str::to_lowercase),
        );
        assert!(sent.insert(name, entry).is_none(), "one sent_file per file");
    }
    let expected: BTreeMap<String, (Option<u64>, Option<String>)> = files
        .iter()
        .map(|(name, bytes)| {
            (
                (*name).to_owned(),
                (Some(*bytes), Some(pattern_sha256(*bytes))),
            )
        })
        .collect();
    assert_eq!(sent, expected, "the sender wrote the agreed pattern");
    let offer = to.wait_event("offer");
    assert_eq!(
        offer["peer_fp"],
        from.fp.as_str(),
        "the offer is the sender's"
    );
    assert_eq!(offer["trusted"], true, "the sender is paired");
    assert_eq!(offer["code"], "", "a paired send shows no code");
    assert_eq!(offer["files"].as_u64(), Some(files.len() as u64));
    assert_eq!(offer["accepted"], true, "the receiver accepted");
    let mut received = BTreeMap::new();
    for _ in files {
        let e = to.wait_event("received");
        let name = e["name"].as_str().unwrap_or("").to_owned();
        let entry = (
            e["bytes"].as_u64(),
            e["sha256"].as_str().map(str::to_lowercase),
        );
        assert!(
            received.insert(name, entry).is_none(),
            "one received per file"
        );
    }
    assert_eq!(received, sent, "every received file matches its sent one");
    let finished = to.wait_event("receive_finished");
    assert_eq!(finished["outcome"], "completed", "{finished}");
    let result = from.wait_event("send_result");
    assert_eq!(result["outcome"], "completed", "{result}");
    assert_eq!(result["error"], "", "{result}");
    assert!(
        !from.saw("code") && !to.saw("code"),
        "no code during a paired send"
    );
}

/// The whole check for one pair A ↔ B (protocol.md, "What the driver
/// checks").
fn pair_and_send(kind_a: Kind, kind_b: Kind) {
    // 1. Launch both (concurrently) and read both hellos.
    let starting_a = Starting::launch(kind_a);
    let starting_b = Starting::launch(kind_b);
    let mut a = starting_a.connect();
    let mut b = starting_b.connect();
    eprintln!(
        "A = {} ({}), B = {} ({})",
        kind_a.label(),
        a.hello["name"],
        kind_b.label(),
        b.hello["name"]
    );
    assert_ne!(a.fp, b.fp, "two distinct identities");
    // 2. Pairing.
    pair(&mut a, &mut b);
    // 3. A → B, paired.
    send_paired(
        &mut a,
        &mut b,
        &[("a-to-b.bin", 3 * 1024 * 1024 + 7), ("a-to-b.txt", 20)],
    );
    // 4. B → A.
    send_paired(
        &mut b,
        &mut a,
        &[("b-to-a.bin", 3 * 1024 * 1024 + 7), ("b-to-a.txt", 20)],
    );
    // 5. End: both tests finish OK.
    a.quit();
    b.quit();
    a.finish();
    b.finish();
}

#[test]
#[ignore = "opt-in: DROPUMI_TEST_DEVICES=1, DROPUMI_TEST_APPLE_DEVICE, DROPUMI_TEST_LAN_ADDR and ANDROID_SERIAL (scripts/devices/interop.sh)"]
fn devices_iphone_android_pair_and_send() {
    if enabled(&[Kind::Iphone, Kind::Android]) {
        pair_and_send(Kind::Iphone, Kind::Android);
    }
}

#[test]
#[ignore = "opt-in: DROPUMI_TEST_DEVICES=1, DROPUMI_TEST_APPLE_DEVICE and DROPUMI_TEST_LAN_ADDR (scripts/devices/interop.sh)"]
fn devices_iphone_mac_pair_and_send() {
    if enabled(&[Kind::Iphone, Kind::Mac]) {
        pair_and_send(Kind::Iphone, Kind::Mac);
    }
}

#[test]
#[ignore = "opt-in: DROPUMI_TEST_DEVICES=1 and ANDROID_SERIAL (scripts/devices/interop.sh)"]
fn devices_android_mac_pair_and_send() {
    if enabled(&[Kind::Android, Kind::Mac]) {
        pair_and_send(Kind::Android, Kind::Mac);
    }
}

#[test]
fn file_pattern_hash_is_the_agents() {
    // Byte i is (i * 31 + 7) % 251: 7, 38, 69, …
    let mut h = Sha256::new();
    h.update([7u8, 38, 69]);
    let want: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(pattern_sha256(3), want);
    assert_eq!(
        pattern_sha256(0),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}
