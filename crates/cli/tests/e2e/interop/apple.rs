// SPDX-License-Identifier: MPL-2.0
//! The Apple app's real core (`AppModel` over `LiveCoreClient`, inside an
//! app-hosted XCTest) against the real CLI over loopback, on macOS and on
//! the iOS simulator.
//!
//! Opt-in and macOS only:
//!
//! ```sh
//! bash scripts/apple/interop.sh        # builds the test bundles, then runs these
//! ```
//!
//! which runs `DROPUMI_TEST_APPLE=1 cargo test -p dropumi-cli --test e2e
//! --locked -- --ignored apple_` after `xcodebuild build-for-testing`.
//!
//! The XCTest (`apps/apple/Tests/Interop/AppleInteropTests.swift`) plays
//! the user. It confirms a code or accepts an offer only if the
//! authenticated fingerprint is the CLI's. This harness then checks that
//! both sides showed the same code and that the bytes arrived intact.
//! Status comes back as `DROPUMI_STATUS {json}` lines in the test output.
//! Loopback only: these tests say nothing about Bonjour, Wi-Fi or OS
//! permission prompts. The `apple_macos_bonjour_*` tests are the
//! exception: they also need `DROPUMI_TEST_MDNS=1` and use the app's native
//! Bonjour adapter against the CLI's mDNS on this Mac's network. The first
//! run shows the macOS Local Network question for Dropumi; someone must
//! choose Allow (a rebuilt, ad-hoc signed app may ask again). They cannot
//! run on the iOS 27.0 simulator: there the app's DNS-SD requests are
//! accepted but never answered (no prompt is shown), while the same calls
//! from a plain test process work.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::srv_target;
use crate::support::*;

#[derive(Clone, Copy, Debug)]
enum Platform {
    MacOs,
    IosSimulator,
}

impl Platform {
    fn scheme(self) -> &'static str {
        match self {
            Self::MacOs => "Dropumi-macOS",
            Self::IosSimulator => "Dropumi-iOS",
        }
    }

    fn destination(self) -> String {
        match self {
            Self::MacOs => "platform=macOS,arch=arm64".into(),
            Self::IosSimulator => format!(
                "platform=iOS Simulator,name={}",
                std::env::var("DROPUMI_TEST_APPLE_SIMULATOR")
                    .unwrap_or_else(|_| "iPhone 17".into())
            ),
        }
    }

    fn test(self) -> &'static str {
        match self {
            Self::MacOs => "DropumiMacTests/AppleInteropTests",
            Self::IosSimulator => "DropumiTests/AppleInteropTests",
        }
    }
}

fn enabled() -> bool {
    if std::env::var("DROPUMI_TEST_APPLE").as_deref() == Ok("1") {
        return true;
    }
    eprintln!("skipped: set DROPUMI_TEST_APPLE=1 (see scripts/apple/interop.sh)");
    false
}

fn mdns_enabled() -> bool {
    if std::env::var("DROPUMI_TEST_MDNS").as_deref() == Ok("1") {
        return true;
    }
    eprintln!("skipped: set DROPUMI_TEST_MDNS=1 as well (needs multicast on a LAN interface)");
    false
}

fn apple_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/apple")
}

/// One run of the app-hosted interop XCTest.
struct AppTest {
    child: Child,
    statuses: Receiver<Value>,
    log: Arc<Mutex<Vec<String>>>,
}

impl AppTest {
    fn start(platform: Platform, scenario: &str, env: &[(&str, &str)]) -> Self {
        let dir = apple_dir();
        assert!(
            dir.join("Dropumi.xcodeproj").exists(),
            "no Xcode project: run scripts/apple/interop.sh"
        );
        let mut cmd = Command::new("xcodebuild");
        cmd.current_dir(&dir)
            .args(["test-without-building", "-project", "Dropumi.xcodeproj"])
            .args(["-scheme", platform.scheme()])
            .args(["-destination", &platform.destination()])
            .args(["-derivedDataPath", "build"])
            .arg(format!("-only-testing:{}", platform.test()))
            // Passed to the test process without the TEST_RUNNER_ prefix.
            .env("TEST_RUNNER_DROPUMI_INTEROP", scenario)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in env {
            cmd.env(format!("TEST_RUNNER_{key}"), value);
        }
        let mut child = cmd.spawn().expect("run xcodebuild");
        let (tx, statuses) = channel();
        let log = Arc::new(Mutex::new(Vec::new()));
        let out = child.stdout.take().expect("stdout");
        let sink = Arc::clone(&log);
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines() {
                let Ok(line) = line else { break };
                if let Some(at) = line.find("DROPUMI_STATUS ")
                    && let Ok(v) = serde_json::from_str::<Value>(&line[at + 15..])
                {
                    let _ = tx.send(v);
                }
                sink.lock().unwrap().push(line);
            }
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
            log,
        }
    }

    fn tail(&self) -> String {
        let log = self.log.lock().unwrap();
        log[log.len().saturating_sub(40)..].join("\n")
    }

    /// Waits for the app's status `event`. The first one allows for the
    /// simulator to boot and the app to launch.
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

    /// Waits for xcodebuild to exit; the XCTest's own assertions must pass.
    fn finish(mut self) {
        let deadline = Instant::now() + EXIT;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "the app's XCTest failed\n{}", self.tail());
                return;
            }
            assert!(
                Instant::now() < deadline,
                "xcodebuild did not exit\n{}",
                self.tail()
            );
            // Polls the process state only; nothing is synchronized on it.
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for AppTest {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Launching the app (and booting the simulator) can take a while.
const LAUNCH: Duration = Duration::from_secs(45);

fn app_sends_to_cli_after_code(platform: Platform) {
    let cli = Device::new("Cli");
    let dst = tempfile::tempdir().unwrap();
    let (mut rx, port) = receiver(&cli, dst.path(), &["--policy", "everyone", "--once"]);
    let address = format!("127.0.0.1:{port}");
    let app = AppTest::start(
        platform,
        "app-sends",
        &[
            ("DROPUMI_CLI_ADDR", &address),
            ("DROPUMI_CLI_FP", &cli.fingerprint),
        ],
    );
    let app_fp = app.wait("ready", LAUNCH)["fingerprint"]
        .as_str()
        .unwrap()
        .to_owned();
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

fn cli_sends_to_app_after_code(platform: Platform) {
    let cli = Device::new("Cli");
    let app = AppTest::start(
        platform,
        "app-receives",
        &[("DROPUMI_CLI_FP", &cli.fingerprint)],
    );
    let app_fp = app.wait("ready", LAUNCH)["fingerprint"]
        .as_str()
        .unwrap()
        .to_owned();
    let port = app.wait("listening", STEP)["port"]
        .as_str()
        .unwrap()
        .parse::<u16>()
        .unwrap();
    let src = tempfile::tempdir().unwrap();
    let f = write_pattern(&src.path().join("from cli.bin"), 2 * 1024 * 1024 + 3, 5);
    let mut tx = sender(
        &cli,
        std::slice::from_ref(&f),
        port,
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
    app.finish();
}

fn pairing_codes_match(platform: Platform) {
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
    let port = l.wait_event("listening")["port"].as_u64().unwrap();
    let address = format!("127.0.0.1:{port}");
    let app = AppTest::start(
        platform,
        "app-pairs",
        &[
            ("DROPUMI_CLI_ADDR", &address),
            ("DROPUMI_CLI_FP", &cli.fingerprint),
        ],
    );
    let app_fp = app.wait("ready", LAUNCH)["fingerprint"]
        .as_str()
        .unwrap()
        .to_owned();
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

fn bonjour_cli_finds_app_at_random_host(platform: Platform) {
    let app = AppTest::start(platform, "bonjour-advertise", &[]);
    let app_fp = app.wait("ready", LAUNCH)["fingerprint"]
        .as_str()
        .unwrap()
        .to_owned();
    let ad = app.wait("advertising", STEP);
    let instance = ad["instance"].as_str().unwrap().to_owned();
    let port: u16 = ad["port"].as_str().unwrap().parse().unwrap();
    assert!(
        instance.starts_with("dropumi-"),
        "random instance name: {instance}"
    );
    // The CLI (mdns-sd) finds the app's advertisement and its key hint.
    let cli = Device::new("Cli");
    let (code, events) = run(cli.config(), &["discover", "--timeout-ms", "4000"]);
    assert_eq!(code, 0);
    assert!(
        events
            .iter()
            .any(|e| e["event"] == "peer_found" && e["fp_hint"] == app_fp[..8]),
        "{events:?}"
    );
    // G2: the advertisement points at a random host name, never this
    // machine's own name.
    let (host, srv_port) = srv_target(&instance);
    assert_eq!(srv_port, port);
    assert!(
        host.starts_with("dropumi-") && host.ends_with(".local."),
        "SRV target must be the random host, got {host}"
    );
    // Paired only: a sender the app has not paired with is refused.
    cli.trust_fp(&app_fp, "App");
    let src = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("refused.txt"), b"not paired");
    let mut tx = sender(&cli, std::slice::from_ref(&f), port, &[]);
    assert_eq!(tx.wait_exit(EXIT), 6, "the app refused the unpaired sender");
    assert_eq!(tx.wait_event("failed")["code"], "not_authorized");
    app.wait("refused", STEP);
    app.finish();
}

fn bonjour_app_finds_cli_and_sends(platform: Platform) {
    let cli = Device::new("Cli Bonjour");
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
    let app = AppTest::start(
        platform,
        "bonjour-browse-and-send",
        &[("DROPUMI_CLI_FP", &cli.fingerprint)],
    );
    let app_fp = app.wait("ready", LAUNCH)["fingerprint"]
        .as_str()
        .unwrap()
        .to_owned();
    let found = app.wait("found", STEP);
    assert_eq!(found["name"], "Cli Bonjour");
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

#[test]
#[ignore = "opt-in: DROPUMI_TEST_APPLE=1 and DROPUMI_TEST_MDNS=1; asks for Local Network access"]
fn apple_macos_bonjour_cli_finds_app_at_random_host() {
    if enabled() && mdns_enabled() {
        bonjour_cli_finds_app_at_random_host(Platform::MacOs);
    }
}

#[test]
#[ignore = "opt-in: DROPUMI_TEST_APPLE=1 and DROPUMI_TEST_MDNS=1; asks for Local Network access"]
fn apple_macos_bonjour_app_finds_cli_and_sends() {
    if enabled() && mdns_enabled() {
        bonjour_app_finds_cli_and_sends(Platform::MacOs);
    }
}

#[test]
#[ignore = "opt-in: DROPUMI_TEST_APPLE=1 (scripts/apple/interop.sh)"]
fn apple_macos_app_sends_to_cli_after_code() {
    if enabled() {
        app_sends_to_cli_after_code(Platform::MacOs);
    }
}

#[test]
#[ignore = "opt-in: DROPUMI_TEST_APPLE=1 (scripts/apple/interop.sh)"]
fn apple_macos_cli_sends_to_app_after_code() {
    if enabled() {
        cli_sends_to_app_after_code(Platform::MacOs);
    }
}

#[test]
#[ignore = "opt-in: DROPUMI_TEST_APPLE=1 (scripts/apple/interop.sh)"]
fn apple_macos_pairing_codes_match() {
    if enabled() {
        pairing_codes_match(Platform::MacOs);
    }
}

#[test]
#[ignore = "opt-in: DROPUMI_TEST_APPLE=1 (scripts/apple/interop.sh)"]
fn apple_ios_sim_app_sends_to_cli_after_code() {
    if enabled() {
        app_sends_to_cli_after_code(Platform::IosSimulator);
    }
}

#[test]
#[ignore = "opt-in: DROPUMI_TEST_APPLE=1 (scripts/apple/interop.sh)"]
fn apple_ios_sim_cli_sends_to_app_after_code() {
    if enabled() {
        cli_sends_to_app_after_code(Platform::IosSimulator);
    }
}

#[test]
#[ignore = "opt-in: DROPUMI_TEST_APPLE=1 (scripts/apple/interop.sh)"]
fn apple_ios_sim_pairing_codes_match() {
    if enabled() {
        pairing_codes_match(Platform::IosSimulator);
    }
}
