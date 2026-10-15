// SPDX-License-Identifier: MPL-2.0
//! Cancellation, crashes, refused connections and cleanup.

use std::time::{Duration, SystemTime};

use crate::support::*;

fn trusted_pair() -> (Device, Device) {
    let a = Device::new("Alice");
    let b = Device::new("Bob");
    a.trust(&b, "Bob");
    b.trust(&a, "Alice");
    (a, b)
}

/// Waits until `dir` has no Dropumi temp files (cleanup is asynchronous).
fn assert_no_parts_soon(dir: &std::path::Path) {
    for _ in 0..100 {
        if !has_part_files(dir) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("partial files left: {:?}", dir_names(dir));
}

#[test]
fn sender_cancels_mid_transfer() {
    let (a, b) = trusted_pair();
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write_pattern(&src.path().join("slow.bin"), 8 * 1024 * 1024, 3);
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted", "--once"]);
    let mut tx = sender(&a, &[f], port, &["--throttle-bps", "1048576"]);
    tx.wait_event("progress");
    tx.cmd("cancel");
    assert_eq!(tx.wait_exit(EXIT), 4, "{}", tx.stderr());
    assert_eq!(rx.wait_exit(EXIT), 4, "{}", rx.stderr());
    assert_no_parts_soon(dst.path());
    assert!(dir_names(dst.path()).is_empty());
}

#[test]
fn receiver_cancels_mid_transfer() {
    let (a, b) = trusted_pair();
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write_pattern(&src.path().join("slow.bin"), 8 * 1024 * 1024, 4);
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted", "--once"]);
    let mut tx = sender(&a, &[f], port, &["--throttle-bps", "1048576"]);
    rx.wait_event("progress");
    rx.cmd("cancel");
    assert_eq!(rx.wait_exit(EXIT), 4, "{}", rx.stderr());
    assert_eq!(tx.wait_exit(EXIT), 4, "{}", tx.stderr());
    assert_eq!(tx.wait_event("cancelled")["by"], "peer");
    assert_no_parts_soon(dst.path());
    assert!(dir_names(dst.path()).is_empty());
}

#[test]
fn sender_killed_receiver_cleans_up() {
    let (a, b) = trusted_pair();
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write_pattern(&src.path().join("slow.bin"), 8 * 1024 * 1024, 5);
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted", "--once"]);
    let mut tx = sender(&a, &[f], port, &["--throttle-bps", "1048576"]);
    rx.wait_event("progress");
    tx.kill();
    assert_eq!(rx.wait_exit(EXIT), 5, "{}", rx.stderr());
    assert_eq!(rx.wait_event("failed")["code"], "connection_lost");
    assert_no_parts_soon(dst.path());
    assert!(dir_names(dst.path()).is_empty());
}

#[test]
fn receiver_killed_sender_reports_connection_lost() {
    let (a, b) = trusted_pair();
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write_pattern(&src.path().join("slow.bin"), 8 * 1024 * 1024, 6);
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted", "--once"]);
    let mut tx = sender(&a, &[f], port, &["--throttle-bps", "1048576"]);
    tx.wait_event("progress");
    rx.kill();
    assert_eq!(tx.wait_exit(EXIT), 5, "{}", tx.stderr());
    assert_eq!(tx.wait_event("failed")["code"], "connection_lost");
}

/// A loopback port that refuses connections. Port 1 is privileged, so no
/// test can bind it; a released ephemeral port is not safe, because another
/// test running in parallel can bind it again before the sender connects.
fn closed_port() -> u16 {
    let port = 1;
    assert!(
        std::net::TcpStream::connect(("127.0.0.1", port)).is_err(),
        "something listens on 127.0.0.1:{port}"
    );
    port
}

#[test]
fn connect_refused_exit_5() {
    let (a, _b) = trusted_pair();
    let src = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("x.txt"), b"x");
    let started = std::time::Instant::now();
    let mut tx = sender(&a, &[f], closed_port(), &[]);
    assert_eq!(tx.wait_exit(EXIT), 5);
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn retry_skips_dead_candidate() {
    let (a, b) = trusted_pair();
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("x.txt"), b"retry");
    let dead = closed_port();
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted", "--once"]);
    let mut tx = Proc::spawn(
        "sender",
        a.config(),
        &args(&[
            "send",
            &f.display().to_string(),
            "--to",
            &format!("127.0.0.1:{dead}"),
            "--to",
            &format!("127.0.0.1:{port}"),
        ]),
    );
    assert_eq!(tx.wait_exit(EXIT), 0, "{}", tx.stderr());
    let order: Vec<String> = tx
        .all("connecting")
        .iter()
        .map(|e| e["addr"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        order,
        vec![format!("127.0.0.1:{dead}"), format!("127.0.0.1:{port}")]
    );
    assert_eq!(rx.wait_exit(EXIT), 0);
}

#[test]
fn stale_parts_cleaned_on_start() {
    let (_a, b) = trusted_pair();
    let dst = tempfile::tempdir().unwrap();
    let old = write(&dst.path().join(".dropumi-abcdef123456.part"), b"old");
    let fresh = write(&dst.path().join(".dropumi-zyxwvu654321.part"), b"fresh");
    let notes = write(&dst.path().join(".dropumi-notes.txt"), b"keep");
    std::fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(3600))
        .unwrap();
    let (mut rx, _port) = receiver(&b, dst.path(), &["--accept", "trusted"]);
    rx.cmd("quit");
    assert_eq!(rx.wait_exit(EXIT), 0);
    assert!(!old.exists());
    assert!(fresh.exists());
    assert!(notes.exists());
}
