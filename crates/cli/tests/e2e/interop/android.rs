// SPDX-License-Identifier: MPL-2.0
//! The Android app's real core (`AppModel` over `LiveCoreClient`, inside the
//! instrumented `AndroidInteropTest` of the minified `qa` build) against the
//! real CLI, on an emulator or a device reached through adb.
//!
//! Opt-in:
//!
//! ```sh
//! bash scripts/android/interop.sh      # builds and installs qa, then runs these
//! ```
//!
//! which runs `DROPUMI_TEST_ANDROID=1 cargo test -p dropumi-cli --test e2e
//! --locked -- --ignored --test-threads=1 android_` after installing the
//! `qa` app and its test APK. `ANDROID_SERIAL` picks the device.
//!
//! Each test starts one scenario with `adb shell am instrument -w -r -e
//! dropumi_app interop -e class io.github.dropumi.interop.AndroidInteropTest#<scenario>`:
//! the app side runs on the interop graph (the real core on temporary
//! folders under the app's `noBackupFilesDir/interop-<uuid>/`) and plays the
//! user. It confirms a code or accepts an offer only if the authenticated
//! fingerprint is the CLI's. Its status comes back live as
//! `INSTRUMENTATION_STATUS: DROPUMI_STATUS={json}` lines. This harness then
//! checks that both sides showed the same code and that the bytes arrived
//! intact.
//!
//! Reachability: the CLI listens on the host's 127.0.0.1 and the app dials
//! 127.0.0.1 on the device through `adb reverse`; the CLI sends to the app
//! through `adb forward`. A guard removes both. Through adb every inbound
//! peer is 127.0.0.1, so per-IP admission is not covered here. The
//! `android_nsd_*` tests are the exception: they need a real device on the
//! host's Wi-Fi and `DROPUMI_TEST_MDNS=1`, and use the app's NSD adapter
//! against the CLI's mDNS.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::support::*;

/// The qa app and its test APK (scripts/android/interop.sh installs both).
const APP_PACKAGE: &str = "io.github.dropumi.qa";
const TEST_PACKAGE: &str = "io.github.dropumi.qa.test";
const RUNNER: &str = "io.github.dropumi.qa.test/io.github.dropumi.DropumiTestRunner";
const TEST_CLASS: &str = "io.github.dropumi.interop.AndroidInteropTest";
const STATUS_PREFIX: &str = "INSTRUMENTATION_STATUS: DROPUMI_STATUS=";

/// Starting the instrumentation and opening the core can take a while.
const LAUNCH: Duration = Duration::from_secs(45);

/// The app's refusal notices (`AppModel.noticeRefused`): without and with
/// the peer.
const REFUSAL_TITLES: [&str; 2] = [
    "A device was refused",
    "A device that isn't paired tried to send",
];

fn enabled() -> bool {
    if std::env::var("DROPUMI_TEST_ANDROID").as_deref() == Ok("1") {
        return true;
    }
    eprintln!("skipped: set DROPUMI_TEST_ANDROID=1 (see scripts/android/interop.sh)");
    false
}

fn mdns_enabled() -> bool {
    if std::env::var("DROPUMI_TEST_MDNS").as_deref() == Ok("1") {
        return true;
    }
    eprintln!("skipped: set DROPUMI_TEST_MDNS=1 as well (a real device on the host's Wi-Fi)");
    false
}

/// adb from `PATH` (scripts/android/interop.sh puts the one it uses first),
/// else from the SDK (`ANDROID_HOME`, `ANDROID_SDK_ROOT`).
fn adb_path() -> PathBuf {
    let on_path = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join("adb"))
            .find(|adb| adb.is_file())
    });
    on_path
        .or_else(|| {
            ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
                .iter()
                .filter_map(std::env::var_os)
                .map(|sdk| PathBuf::from(sdk).join("platform-tools").join("adb"))
                .find(|adb| adb.is_file())
        })
        .unwrap_or_else(|| PathBuf::from("adb"))
}

/// An adb command for the device `ANDROID_SERIAL` names (inherited).
fn adb() -> Command {
    let mut cmd = Command::new(adb_path());
    cmd.stdin(Stdio::null());
    cmd
}

/// Runs adb to completion; it must succeed. Returns its stdout.
fn adb_ok(args: &[&str]) -> String {
    let Output {
        status,
        stdout,
        stderr,
    } = adb().args(args).output().expect("run adb");
    assert!(
        status.success(),
        "adb {args:?} failed: {}",
        String::from_utf8_lossy(&stderr)
    );
    String::from_utf8_lossy(&stdout).replace('\r', "")
}

/// Whether the device is an emulator (the NSD tests need a real device).
fn on_emulator() -> bool {
    ["ro.kernel.qemu", "ro.boot.qemu"]
        .iter()
        .any(|prop| adb_ok(&["shell", "getprop", prop]).trim() == "1")
}

/// What a missing shared Downloads folder lists as.
const DOWNLOADS_ABSENT: &str = "__absent__";

/// The entries of the device's shared Downloads folder (hidden ones too).
/// A listing that fails (no shared storage, not readable) fails the test;
/// a missing folder lists as [`DOWNLOADS_ABSENT`], so a folder that
/// appears is a change too.
fn shared_downloads() -> BTreeSet<String> {
    let list = format!(
        "[ -d /sdcard/ ] || {{ echo 'no shared storage at /sdcard' >&2; exit 3; }}; \
         if [ -e /sdcard/Download ]; then ls -1a /sdcard/Download/; else echo {DOWNLOADS_ABSENT}; fi"
    );
    adb_ok(&["shell", &list])
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// adb port forwards for one test; removed when dropped.
#[derive(Default)]
struct Forwards {
    forwards: Vec<u16>,
    reverses: Vec<u16>,
}

impl Forwards {
    /// CLI → app: a host port that adb forwards to the app's `port`.
    fn forward_to_app(&mut self, port: u16) -> u16 {
        let out = adb_ok(&["forward", "tcp:0", &format!("tcp:{port}")]);
        let host: u16 = out
            .trim()
            .parse()
            .expect("adb forward prints the host port");
        self.forwards.push(host);
        host
    }

    /// App → CLI: the device's 127.0.0.1:`port` reaches the host's.
    fn reverse_to_cli(&mut self, port: u16) {
        let spec = format!("tcp:{port}");
        adb_ok(&["reverse", &spec, &spec]);
        self.reverses.push(port);
    }
}

impl Drop for Forwards {
    fn drop(&mut self) {
        for port in &self.forwards {
            let _ = adb()
                .args(["forward", "--remove", &format!("tcp:{port}")])
                .output();
        }
        for port in &self.reverses {
            let _ = adb()
                .args(["reverse", "--remove", &format!("tcp:{port}")])
                .output();
        }
    }
}

/// One run of one `AndroidInteropTest` scenario.
struct AppTest {
    child: Child,
    statuses: Receiver<Value>,
    /// Signalled when the instrumentation's output ends.
    ended: Receiver<()>,
    log: Arc<Mutex<Vec<String>>>,
}

impl AppTest {
    fn start(scenario: &str, arguments: &[(&str, &str)]) -> Self {
        assert!(
            adb_ok(&["shell", "pm", "list", "packages", TEST_PACKAGE])
                .lines()
                .any(|l| l.trim() == format!("package:{TEST_PACKAGE}")),
            "the qa test APK is not installed: run scripts/android/interop.sh"
        );
        let class = format!("{TEST_CLASS}#{scenario}");
        let mut cmd = adb();
        cmd.args(["shell", "am", "instrument", "-w", "-r"]).args([
            "-e",
            "dropumi_app",
            "interop",
            "-e",
            "class",
            &class,
        ]);
        for (key, value) in arguments {
            // adb shell joins its arguments into one device shell line.
            assert!(
                value
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || ".:_-".contains(c)),
                "unsafe runner argument {key}"
            );
            cmd.args(["-e", key, value]);
        }
        cmd.arg(RUNNER)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("run adb");
        let (tx, statuses) = channel();
        let (done, ended) = channel();
        let log = Arc::new(Mutex::new(Vec::new()));
        let out = child.stdout.take().expect("stdout");
        let sink = Arc::clone(&log);
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines() {
                let Ok(line) = line else { break };
                let line = line.trim_end_matches('\r').to_owned();
                if let Some(json) = line.strip_prefix(STATUS_PREFIX)
                    && let Ok(v) = serde_json::from_str::<Value>(json)
                {
                    let _ = tx.send(v);
                }
                sink.lock().unwrap().push(line);
            }
            let _ = done.send(());
        });
        let err = child.stderr.take().expect("stderr");
        let sink = Arc::clone(&log);
        std::thread::spawn(move || {
            for line in BufReader::new(err).lines() {
                let Ok(line) = line else { break };
                sink.lock().unwrap().push(line);
            }
        });
        Self {
            child,
            statuses,
            ended,
            log,
        }
    }

    fn tail(&self) -> String {
        let log = self.log.lock().unwrap();
        log[log.len().saturating_sub(40)..].join("\n")
    }

    /// Waits for the app's status `event`.
    fn wait(&self, event: &str, timeout: Duration) -> Value {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.statuses.recv_timeout(left) {
                Ok(v) if v["event"] == event => return v,
                Ok(v) if v["event"] == "timeout" => {
                    panic!("app timed out: {v}\n{}", self.tail())
                }
                Ok(_) => {}
                Err(_) => panic!("timed out waiting for app status {event}\n{}", self.tail()),
            }
        }
    }

    /// The `ready` status: the app's fingerprint, also printed with the API
    /// level and ABI for the result label.
    fn ready(&self) -> String {
        let ready = self.wait("ready", LAUNCH);
        eprintln!(
            "app ready: API {}, {}",
            ready["sdk"].as_str().unwrap_or("?"),
            ready["abi"].as_str().unwrap_or("?")
        );
        ready["fingerprint"].as_str().unwrap().to_owned()
    }

    /// Waits for the instrumentation to end; the scenario's own assertions
    /// must pass (`am instrument` exits 0 either way, so its report is read).
    fn finish(mut self) {
        assert!(
            self.ended.recv_timeout(EXIT).is_ok(),
            "the instrumentation did not end\n{}",
            self.tail()
        );
        let _ = self.child.wait();
        let log = self.log.lock().unwrap().join("\n");
        // Status codes -1 to -4: an error, a failure, an ignored test and a
        // failed assumption (a skip is never a pass).
        let failed = [
            "FAILURES!!!",
            "INSTRUMENTATION_FAILED",
            "INSTRUMENTATION_STATUS_CODE: -1",
            "INSTRUMENTATION_STATUS_CODE: -2",
            "INSTRUMENTATION_STATUS_CODE: -3",
            "INSTRUMENTATION_STATUS_CODE: -4",
            "Process crashed",
        ]
        .iter()
        .any(|bad| log.contains(bad));
        assert!(
            log.contains("OK (1 test)") && !failed,
            "the app's test failed\n{}",
            self.tail()
        );
    }
}

impl Drop for AppTest {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Ends an instrumentation that is still running on the device.
        let _ = adb()
            .args(["shell", "am", "force-stop", APP_PACKAGE])
            .output();
    }
}

fn app_sends_to_cli_after_code() {
    let cli = Device::new("Cli");
    let dst = tempfile::tempdir().unwrap();
    let (mut rx, port) = receiver(&cli, dst.path(), &["--policy", "everyone", "--once"]);
    let mut forwards = Forwards::default();
    forwards.reverse_to_cli(port);
    let address = format!("127.0.0.1:{port}");
    let app = AppTest::start(
        "appSendsToCli",
        &[
            ("dropumi_cli_addr", &address),
            ("dropumi_cli_fp", &cli.fingerprint),
        ],
    );
    let app_fp = app.ready();
    let file = app.wait("file", STEP);
    let app_code = app.wait("code", STEP);
    assert_eq!(app_code["matches"], "true", "the app saw the CLI's key");
    let offer = rx.wait_event("offer_received");
    assert_eq!(offer["peer"]["fingerprint"], app_fp.as_str());
    assert_eq!(offer["peer"]["trusted"], false);
    assert_eq!(
        offer["sas"], app_code["code"],
        "both sides show the same code"
    );
    rx.cmd("accept");
    assert_eq!(rx.wait_exit(EXIT), 0, "{}", rx.stderr());
    assert_eq!(app.wait("finished", STEP)["outcome"], "completed");
    assert_eq!(
        sha256(&dst.path().join(file["name"].as_str().unwrap())),
        file["sha256"].as_str().unwrap()
    );
    app.finish();
    assert!(cli.trusted().is_empty(), "nothing was paired");
}

fn app_sends_content_uri_to_cli() {
    let cli = Device::new("Cli");
    let dst = tempfile::tempdir().unwrap();
    let (mut rx, port) = receiver(&cli, dst.path(), &["--policy", "everyone", "--once"]);
    let mut forwards = Forwards::default();
    forwards.reverse_to_cli(port);
    let address = format!("127.0.0.1:{port}");
    let app = AppTest::start(
        "appSendsContentUri",
        &[
            ("dropumi_cli_addr", &address),
            ("dropumi_cli_fp", &cli.fingerprint),
        ],
    );
    let app_fp = app.ready();
    // The name the other app's provider reports, read through UriGuard.
    let file = app.wait("file", STEP);
    assert_eq!(file["name"], "notes.txt");
    let app_code = app.wait("code", STEP);
    assert_eq!(app_code["matches"], "true", "the app saw the CLI's key");
    let offer = rx.wait_event("offer_received");
    assert_eq!(offer["peer"]["fingerprint"], app_fp.as_str());
    assert_eq!(
        offer["sas"], app_code["code"],
        "both sides show the same code"
    );
    rx.cmd("accept");
    assert_eq!(rx.wait_exit(EXIT), 0, "{}", rx.stderr());
    assert_eq!(app.wait("finished", STEP)["outcome"], "completed");
    assert_eq!(
        sha256(&dst.path().join("notes.txt")),
        file["sha256"].as_str().unwrap()
    );
    app.finish();
}

fn cli_sends_to_app_after_code() {
    let cli = Device::new("Cli");
    let downloads = shared_downloads();
    let app = AppTest::start("cliSendsToApp", &[("dropumi_cli_fp", &cli.fingerprint)]);
    let app_fp = app.ready();
    let port = app.wait("listening", STEP)["port"]
        .as_str()
        .unwrap()
        .parse::<u16>()
        .unwrap();
    let mut forwards = Forwards::default();
    let host_port = forwards.forward_to_app(port);
    let src = tempfile::tempdir().unwrap();
    let f = write_pattern(&src.path().join("from cli.bin"), 2 * 1024 * 1024 + 3, 5);
    let mut tx = sender(
        &cli,
        std::slice::from_ref(&f),
        host_port,
        &["--allow-unverified", "--expect-fingerprint", &app_fp],
    );
    let cli_code = tx.wait_event("sas")["code"].as_str().unwrap().to_owned();
    tx.cmd("confirm");
    let offer = app.wait("offer", STEP);
    assert_eq!(
        offer["code"],
        cli_code.as_str(),
        "both sides show the same code"
    );
    assert_eq!(offer["matches"], "true", "the app saw the CLI's key");
    assert_eq!(offer["file"], "from cli.bin");
    assert_eq!(tx.wait_exit(EXIT), 0, "{}", tx.stderr());
    let finished = app.wait("finished", STEP);
    assert_eq!(finished["outcome"], "completed");
    assert_eq!(finished["sha256"], sha256(&f).as_str());
    assert_eq!(finished["part_files"], "0");
    // The saved file is the receive folder's own entry, not a link, the
    // app's, 0600 in a 0700 folder of the app's (AndroidInteropTest's
    // savedFileFacts).
    for (fact, want) in [
        ("in_receive_dir", "true"),
        ("regular_file", "true"),
        ("file_mode", "600"),
        ("file_owner_is_app", "true"),
        ("dir_is_folder", "true"),
        ("dir_mode", "700"),
        ("dir_owner_is_app", "true"),
    ] {
        assert_eq!(finished[fact], want, "{fact}: {finished}");
    }
    app.finish();
    assert_eq!(
        shared_downloads(),
        downloads,
        "nothing reached the shared Downloads folder"
    );
}

fn pairing_codes_match() {
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
    let mut forwards = Forwards::default();
    forwards.reverse_to_cli(port);
    let address = format!("127.0.0.1:{port}");
    let app = AppTest::start(
        "pairingCodesMatch",
        &[
            ("dropumi_cli_addr", &address),
            ("dropumi_cli_fp", &cli.fingerprint),
        ],
    );
    let app_fp = app.ready();
    let cli_code = l.wait_event("sas")["code"].as_str().unwrap().to_owned();
    let app_code = app.wait("code", STEP);
    assert_eq!(
        app_code["code"],
        cli_code.as_str(),
        "both sides show the same code"
    );
    assert_eq!(app_code["matches"], "true");
    l.cmd("confirm");
    assert_eq!(l.wait_exit(EXIT), 0, "{}", l.stderr());
    let paired = app.wait("paired", STEP);
    assert_eq!(paired["paired"], "true");
    assert_eq!(paired["trusted"], cli.fingerprint.as_str());
    app.finish();
    assert_eq!(cli.trusted(), vec![app_fp]);
}

/// The CLI (which trusts the app) sends to the app's `port`; the app
/// (Paired only) refuses it.
fn refused_by_app(app: &AppTest, app_fp: &str, port: u16) {
    let cli = Device::new("Cli");
    cli.trust_fp(app_fp, "App");
    let mut forwards = Forwards::default();
    let host_port = forwards.forward_to_app(port);
    let src = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("refused.txt"), b"not paired");
    let mut tx = sender(&cli, std::slice::from_ref(&f), host_port, &[]);
    assert_eq!(tx.wait_exit(EXIT), 6, "the app refused the unpaired sender");
    assert_eq!(tx.wait_event("failed")["code"], "not_authorized");
    // The refusal notice itself (AppModel.noticeRefused), not any banner;
    // no prompt or transfer on the way to it.
    let refused = app.wait("refused", STEP);
    assert!(
        REFUSAL_TITLES.contains(&refused["title"].as_str().unwrap_or("")),
        "the app showed the refusal notice: {refused}"
    );
    assert_eq!(
        refused["prompted"], "false",
        "no prompt for the refused sender"
    );
    assert_eq!(
        refused["transferred"], "false",
        "no transfer from the refused sender"
    );
}

fn unpaired_sender_refused() {
    let app = AppTest::start("unpairedSenderRefused", &[]);
    let app_fp = app.ready();
    let port = app.wait("listening", STEP)["port"]
        .as_str()
        .unwrap()
        .parse::<u16>()
        .unwrap();
    refused_by_app(&app, &app_fp, port);
    app.finish();
}

fn nsd_cli_finds_app_at_random_host() {
    let app = AppTest::start("nsdAdvertise", &[]);
    let app_fp = app.ready();
    let ad = app.wait("advertising", STEP);
    let instance = ad["instance"].as_str().unwrap().to_owned();
    let port: u16 = ad["port"].as_str().unwrap().parse().unwrap();
    let cli = Device::new("Cli");
    let (code, events) = run(cli.config(), &["discover", "--timeout-ms", "4000"]);
    assert_eq!(code, 0);
    let found = events
        .iter()
        .any(|e| e["event"] == "peer_found" && e["fp_hint"] == app_fp[..8]);
    if ad["gate_open"] == "true" {
        // The CLI (mdns-sd) finds the app's advertisement and its key hint.
        assert!(found, "{events:?}");
        assert!(
            instance.starts_with("dropumi-"),
            "random instance name: {instance}"
        );
        srv_host_is_not_the_device_name(&instance, port);
    } else {
        // R21 gate closed: browse-only, never advertised.
        assert!(
            !found,
            "the app advertised with the gate closed: {events:?}"
        );
        assert_eq!(instance, "", "no advertisement with the gate closed");
    }
    refused_by_app(&app, &app_fp, port);
    app.finish();
}

/// G2: the advertisement points at a host name that is not the device's
/// own name or model.
#[cfg(target_os = "macos")]
fn srv_host_is_not_the_device_name(instance: &str, port: u16) {
    let (host, srv_port) = super::srv_target(instance);
    assert_eq!(srv_port, port);
    assert!(host.ends_with(".local."), "SRV target: {host}");
    let label = host.to_lowercase();
    for name in [
        adb_ok(&["shell", "settings", "get", "global", "device_name"]),
        adb_ok(&["shell", "getprop", "ro.product.model"]),
    ] {
        let name = name.trim().to_lowercase();
        assert!(
            name.is_empty() || name == "null" || !label.contains(&name.replace(' ', "-")),
            "the SRV target names the device: {host}"
        );
    }
}

#[cfg(not(target_os = "macos"))]
fn srv_host_is_not_the_device_name(_instance: &str, _port: u16) {
    panic!("the SRV host check reads the advertisement with dns-sd: run it on a macOS host");
}

fn nsd_app_finds_cli_and_sends() {
    let cli = Device::new("Cli Nsd");
    let dst = tempfile::tempdir().unwrap();
    let mut rx = Proc::spawn(
        "receiver",
        cli.config(),
        &args(&[
            "receive",
            "--port",
            "0",
            "--policy",
            "everyone",
            "--once",
            "--dir",
            &dst.path().display().to_string(),
        ]),
    );
    rx.wait_event("listening");
    rx.wait_event("advertising");
    let app = AppTest::start("nsdBrowseAndSend", &[("dropumi_cli_fp", &cli.fingerprint)]);
    let app_fp = app.ready();
    let found = app.wait("found", STEP);
    assert_eq!(found["name"], "Cli Nsd");
    assert_eq!(found["compatible"], "true");
    assert_eq!(
        found["looks_like"], "",
        "an unpaired device never looks paired"
    );
    let file = app.wait("file", STEP);
    let app_code = app.wait("code", STEP);
    assert_eq!(app_code["matches"], "true");
    let offer = rx.wait_event("offer_received");
    assert_eq!(offer["peer"]["fingerprint"], app_fp.as_str());
    assert_eq!(
        offer["sas"], app_code["code"],
        "both sides show the same code"
    );
    rx.cmd("accept");
    assert_eq!(rx.wait_exit(EXIT), 0, "{}", rx.stderr());
    assert_eq!(app.wait("finished", STEP)["outcome"], "completed");
    assert_eq!(
        sha256(&dst.path().join(file["name"].as_str().unwrap())),
        file["sha256"].as_str().unwrap()
    );
    app.finish();
}

/// The NSD tests run only on a real device (an emulator's network has no
/// multicast to the host).
fn on_device() -> bool {
    if on_emulator() {
        eprintln!("skipped: needs a real device on the host's Wi-Fi, not an emulator");
        return false;
    }
    true
}

#[test]
#[ignore = "opt-in: DROPUMI_TEST_ANDROID=1 (scripts/android/interop.sh)"]
fn android_app_sends_to_cli_after_code() {
    if enabled() {
        app_sends_to_cli_after_code();
    }
}

#[test]
#[ignore = "opt-in: DROPUMI_TEST_ANDROID=1 (scripts/android/interop.sh)"]
fn android_cli_sends_to_app_after_code() {
    if enabled() {
        cli_sends_to_app_after_code();
    }
}

#[test]
#[ignore = "opt-in: DROPUMI_TEST_ANDROID=1 (scripts/android/interop.sh)"]
fn android_pairing_codes_match() {
    if enabled() {
        pairing_codes_match();
    }
}

#[test]
#[ignore = "opt-in: DROPUMI_TEST_ANDROID=1 (scripts/android/interop.sh)"]
fn android_app_sends_content_uri_to_cli() {
    if enabled() {
        app_sends_content_uri_to_cli();
    }
}

#[test]
#[ignore = "opt-in: DROPUMI_TEST_ANDROID=1 (scripts/android/interop.sh)"]
fn android_unpaired_sender_refused() {
    if enabled() {
        unpaired_sender_refused();
    }
}

#[test]
#[ignore = "opt-in: DROPUMI_TEST_ANDROID=1 and DROPUMI_TEST_MDNS=1; a real device on the host's Wi-Fi"]
fn android_nsd_cli_finds_app_at_random_host() {
    if enabled() && mdns_enabled() && on_device() {
        nsd_cli_finds_app_at_random_host();
    }
}

#[test]
#[ignore = "opt-in: DROPUMI_TEST_ANDROID=1 and DROPUMI_TEST_MDNS=1; a real device on the host's Wi-Fi"]
fn android_nsd_app_finds_cli_and_sends() {
    if enabled() && mdns_enabled() && on_device() {
        nsd_app_finds_cli_and_sends();
    }
}
