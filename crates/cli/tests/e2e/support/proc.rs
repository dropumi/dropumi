// SPDX-License-Identifier: MPL-2.0
//! Spawning and driving real `dropumi` processes.
//!
//! Every process gets its own temporary config directory and `--json
//! --control-stdin`; tests synchronize on NDJSON events, never on sleeps.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

/// Default per-step deadline.
pub const STEP: Duration = Duration::from_secs(30);

/// Path of the binary under test.
#[must_use]
pub fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_dropumi")
}

/// A running `dropumi` process.
pub struct Proc {
    pub name: String,
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<Value>,
    pub seen: Vec<Value>,
    stderr: Arc<Mutex<String>>,
    stderr_thread: Option<std::thread::JoinHandle<()>>,
}

impl Proc {
    /// Starts `dropumi --config-dir CONFIG --json --control-stdin ARGS…`.
    pub fn spawn(name: &str, config: &Path, args: &[String]) -> Self {
        let mut child = Command::new(bin())
            .arg("--config-dir")
            .arg(config)
            .arg("--json")
            .arg("--control-stdin")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("DROPUMI_LOG", "warn")
            .spawn()
            .expect("spawn dropumi");
        let stdout = child.stdout.take().expect("stdout");
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                match serde_json::from_str::<Value>(&line) {
                    Ok(v) => {
                        if tx.send(v).is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        let _ = tx.send(serde_json::json!({"event": "non_json", "line": line}));
                    }
                }
            }
        });
        let stderr = Arc::new(Mutex::new(String::new()));
        let mut err_pipe = child.stderr.take().expect("stderr");
        let sink = Arc::clone(&stderr);
        let stderr_thread = std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = err_pipe.read_to_string(&mut buf);
            sink.lock().unwrap().push_str(&buf);
        });
        let stdin = child.stdin.take();
        Self {
            name: name.into(),
            child,
            stdin,
            rx,
            seen: Vec::new(),
            stderr,
            stderr_thread: Some(stderr_thread),
        }
    }

    /// Pulls every already-available event.
    pub fn pump(&mut self) {
        while let Ok(v) = self.rx.try_recv() {
            self.seen.push(v);
        }
    }

    /// Waits for the first event matching `pred` (including earlier ones).
    pub fn wait_where(
        &mut self,
        what: &str,
        timeout: Duration,
        pred: impl Fn(&Value) -> bool,
    ) -> Value {
        let deadline = Instant::now() + timeout;
        loop {
            self.pump();
            if let Some(v) = self.seen.iter().find(|v| pred(v)) {
                return v.clone();
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                self.fail(&format!("timed out waiting for {what}"));
            }
            match self.rx.recv_timeout(left.min(Duration::from_millis(200))) {
                Ok(v) => self.seen.push(v),
                Err(RecvTimeoutError::Timeout) => {}
                // Stdout is closed and fully read.
                Err(RecvTimeoutError::Disconnected) => {
                    self.fail(&format!("process ended before {what}"));
                }
            }
        }
    }

    /// Waits until at least `n` events match `pred` (including earlier
    /// ones) and returns how many do.
    pub fn wait_count(&mut self, what: &str, n: usize, pred: impl Fn(&Value) -> bool) -> usize {
        let deadline = Instant::now() + STEP;
        loop {
            self.pump();
            let count = self.seen.iter().filter(|v| pred(v)).count();
            if count >= n {
                return count;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                self.fail(&format!(
                    "timed out waiting for {what} (have {count}, want {n})"
                ));
            }
            match self.rx.recv_timeout(left) {
                Ok(v) => self.seen.push(v),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.fail(&format!(
                        "process ended before {what} (have {count}, want {n})"
                    ));
                }
            }
        }
    }

    /// Reads stdout until the process closes it (or `timeout` passes).
    fn drain_until_closed(&mut self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(v) => self.seen.push(v),
                Err(_) => return,
            }
        }
    }

    /// Waits for an event by name.
    pub fn wait_event(&mut self, event: &str) -> Value {
        self.wait_where(event, STEP, |v| v["event"] == event)
    }

    /// Whether an event with this name was seen.
    pub fn saw(&mut self, event: &str) -> bool {
        self.pump();
        self.seen.iter().any(|v| v["event"] == event)
    }

    /// All events with this name.
    pub fn all(&mut self, event: &str) -> Vec<Value> {
        self.pump();
        self.seen
            .iter()
            .filter(|v| v["event"] == event)
            .cloned()
            .collect()
    }

    /// Writes a control command.
    pub fn cmd(&mut self, cmd: &str) {
        if let Some(stdin) = self.stdin.as_mut() {
            let _ = writeln!(stdin, "{{\"cmd\":\"{cmd}\"}}");
            let _ = stdin.flush();
        }
    }

    /// Writes a control command bound to one offer.
    pub fn cmd_for(&mut self, cmd: &str, transfer_id: &str) {
        if let Some(stdin) = self.stdin.as_mut() {
            let line = serde_json::json!({ "cmd": cmd, "transfer_id": transfer_id });
            let _ = writeln!(stdin, "{line}");
            let _ = stdin.flush();
        }
    }

    /// Sends SIGINT (what Ctrl-C sends).
    #[cfg(unix)]
    pub fn interrupt(&mut self) {
        // The shell's built-in `kill`: a standalone kill binary is not
        // installed everywhere (for example in slim container images).
        let status = Command::new("sh")
            .arg("-c")
            .arg(format!("kill -INT {}", self.child.id()))
            .status()
            .expect("run sh");
        assert!(status.success());
    }

    /// Waits for the process to exit and returns its code.
    pub fn wait_exit(&mut self, timeout: Duration) -> i32 {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait().expect("try_wait") {
                // Collect the last lines: the reader ends when stdout closes.
                self.drain_until_closed(STEP);
                return status.code().unwrap_or(-1);
            }
            if Instant::now() >= deadline {
                self.fail("timed out waiting for exit");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Kills the process.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// OS process id.
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Captured stderr so far.
    #[must_use]
    pub fn stderr(&self) -> String {
        self.stderr.lock().unwrap().clone()
    }

    fn fail(&mut self, why: &str) -> ! {
        self.kill();
        self.drain_until_closed(Duration::from_secs(5));
        if let Some(t) = self.stderr_thread.take() {
            let _ = t.join();
        }
        panic!(
            "[{}] {why}\n--- events ---\n{}\n--- stderr ---\n{}",
            self.name,
            self.seen
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
            self.stderr()
        );
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Runs a short command to completion: (exit code, events).
pub fn run(config: &Path, args: &[&str]) -> (i32, Vec<Value>) {
    let out = Command::new(bin())
        .arg("--config-dir")
        .arg(config)
        .arg("--json")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run dropumi");
    let events = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    (out.status.code().unwrap_or(-1), events)
}

/// A device: a config directory with a named identity.
pub struct Device {
    pub dir: tempfile::TempDir,
    pub fingerprint: String,
    pub public_key: String,
    pub name: String,
}

impl Device {
    /// Creates an identity with `name`.
    pub fn new(name: &str) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let (code, events) = run(dir.path(), &["identity", "set-name", name]);
        assert_eq!(code, 0, "identity set-name: {events:?}");
        let id = events
            .iter()
            .find(|e| e["event"] == "identity")
            .expect("identity event")
            .clone();
        Self {
            fingerprint: id["fingerprint"].as_str().unwrap().into(),
            public_key: id["public_key"].as_str().unwrap().into(),
            name: name.into(),
            dir,
        }
    }

    /// Config directory.
    #[must_use]
    pub fn config(&self) -> &Path {
        self.dir.path()
    }

    /// This device trusts `fp` under `alias`.
    pub fn trust_fp(&self, fp: &str, alias: &str) {
        let (code, events) = run(self.config(), &["trust", "add", fp, "--name", alias]);
        assert_eq!(code, 0, "trust add: {events:?}");
    }

    /// This device trusts `other`.
    pub fn trust(&self, other: &Device, alias: &str) {
        self.trust_fp(&other.fingerprint, alias);
    }

    /// Trusted fingerprints.
    pub fn trusted(&self) -> Vec<String> {
        let (_, events) = run(self.config(), &["trust", "list"]);
        events
            .iter()
            .find(|e| e["event"] == "trusted_devices")
            .map(|e| {
                e["devices"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|d| d["fingerprint"].as_str().unwrap().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Strings helper.
pub fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

/// Starts a loopback receiver and returns it with its port.
pub fn receiver(dev: &Device, dir: &Path, extra: &[&str]) -> (Proc, u16) {
    let mut a = args(&[
        "receive",
        "--bind",
        "127.0.0.1",
        "--port",
        "0",
        "--no-mdns",
        "--dir",
    ]);
    a.push(dir.display().to_string());
    a.extend(args(extra));
    let mut p = Proc::spawn(&format!("receiver {}", dev.name), dev.config(), &a);
    let port = p.wait_event("listening")["port"].as_u64().unwrap();
    (p, u16::try_from(port).unwrap())
}

/// Starts a sender.
pub fn sender(dev: &Device, files: &[PathBuf], port: u16, extra: &[&str]) -> Proc {
    let mut a = vec!["send".to_owned()];
    a.extend(files.iter().map(|f| f.display().to_string()));
    a.push("--to".into());
    a.push(format!("127.0.0.1:{port}"));
    a.extend(args(extra));
    Proc::spawn(&format!("sender {}", dev.name), dev.config(), &a)
}
