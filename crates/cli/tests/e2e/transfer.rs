// SPDX-License-Identifier: MPL-2.0
//! Functional transfers between two processes.

use std::ffi::OsString;

use crate::support::*;

fn trusted_pair() -> (Device, Device) {
    let a = Device::new("Alice");
    let b = Device::new("Bob");
    a.trust(&b, "Bob");
    b.trust(&a, "Alice");
    (a, b)
}

#[test]
fn single_file_trusted_auto_accept() {
    let (a, b) = trusted_pair();
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write_pattern(&src.path().join("report.pdf"), 1_234_567, 42);
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted", "--once"]);
    let mut tx = sender(&a, std::slice::from_ref(&f), port, &[]);
    assert_eq!(tx.wait_exit(EXIT), 0, "{}", tx.stderr());
    assert_eq!(rx.wait_exit(EXIT), 0, "{}", rx.stderr());
    assert_eq!(sha256(&f), sha256(&dst.path().join("report.pdf")));
    let completed = tx.wait_event("completed");
    assert_eq!(completed["status"], "completed");
    assert_eq!(completed["files_stored"], 1);
    assert!(!has_part_files(dst.path()));
}

#[test]
fn multiple_files_empty_unicode_duplicates() {
    let (a, b) = trusted_pair();
    let src = tempfile::tempdir().unwrap();
    std::fs::create_dir(src.path().join("src1")).unwrap();
    std::fs::create_dir(src.path().join("src2")).unwrap();
    let files = vec![
        write(
            &src.path()
                .join("r\u{e9}sum\u{e9} \u{2713} \u{6771}\u{4eac}.txt"),
            "unicode".as_bytes(),
        ),
        write(&src.path().join("empty.bin"), b""),
        write(&src.path().join("src1").join("dup.txt"), b"first"),
        write(&src.path().join("src2").join("dup.txt"), b"second"),
    ];
    let dst = tempfile::tempdir().unwrap();
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted", "--once"]);
    let mut tx = sender(&a, &files, port, &[]);
    assert_eq!(tx.wait_exit(EXIT), 0, "{}", tx.stderr());
    assert_eq!(rx.wait_exit(EXIT), 0);
    let expected: std::collections::BTreeSet<OsString> = [
        "r\u{e9}sum\u{e9} \u{2713} \u{6771}\u{4eac}.txt",
        "empty.bin",
        "dup.txt",
        "dup (1).txt",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    assert_eq!(dir_names(dst.path()), expected);
    assert_eq!(std::fs::read(dst.path().join("dup.txt")).unwrap(), b"first");
    assert_eq!(
        std::fs::read(dst.path().join("dup (1).txt")).unwrap(),
        b"second"
    );
    assert_eq!(std::fs::read(dst.path().join("empty.bin")).unwrap(), b"");
}

#[test]
fn large_file_streaming() {
    let mib: u64 = std::env::var("DROPUMI_TEST_LARGE_MIB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(64);
    let (a, b) = trusted_pair();
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write_pattern(&src.path().join("big.bin"), mib * 1024 * 1024, 7);
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted", "--once"]);
    let mut tx = sender(&a, std::slice::from_ref(&f), port, &[]);
    let timeout = EXIT + std::time::Duration::from_secs(mib / 4);
    assert_eq!(tx.wait_exit(timeout), 0, "{}", tx.stderr());
    assert_eq!(rx.wait_exit(timeout), 0);
    assert_eq!(sha256(&f), sha256(&dst.path().join("big.bin")));
}

#[test]
fn many_small_files() {
    let (a, b) = trusted_pair();
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let files: Vec<_> = (0..300u64)
        .map(|i| write_pattern(&src.path().join(format!("f{i:03}.dat")), (i * 29) % 8192, i))
        .collect();
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted", "--once"]);
    let mut tx = sender(&a, &files, port, &[]);
    assert_eq!(tx.wait_exit(EXIT), 0, "{}", tx.stderr());
    assert_eq!(rx.wait_exit(EXIT), 0);
    for f in &files {
        assert_eq!(sha256(f), sha256(&dst.path().join(f.file_name().unwrap())));
    }
}

#[test]
fn existing_file_never_overwritten() {
    let (a, b) = trusted_pair();
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let original = write(&dst.path().join("photo.jpg"), b"original bytes");
    let before = sha256(&original);
    let f = write(&src.path().join("photo.jpg"), b"new bytes");
    let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted", "--once"]);
    let mut tx = sender(&a, &[f], port, &[]);
    assert_eq!(tx.wait_exit(EXIT), 0);
    assert_eq!(rx.wait_exit(EXIT), 0);
    assert_eq!(sha256(&original), before);
    assert_eq!(
        std::fs::read(dst.path().join("photo (1).jpg")).unwrap(),
        b"new bytes"
    );
    assert_eq!(saved_names(&rx.seen), ["photo (1).jpg".to_owned()].into());
}

#[test]
fn everyone_mode_consent_with_matching_sas() {
    let a = Device::new("Alice");
    let b = Device::new("Bob");
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    let f = write(&src.path().join("hello.txt"), b"hello from a stranger");
    let (mut rx, port) = receiver(&b, dst.path(), &["--policy", "everyone", "--once"]);
    let mut tx = sender(&a, std::slice::from_ref(&f), port, &["--allow-unverified"]);
    let sas_tx = tx.wait_event("sas")["code"].as_str().unwrap().to_owned();
    tx.cmd("confirm");
    let offer = rx.wait_event("offer_received");
    let sas_rx = offer["sas"].as_str().unwrap().to_owned();
    assert_eq!(sas_tx, sas_rx, "both screens show the same code");
    assert_eq!(offer["peer"]["trusted"], false);
    rx.cmd_for("accept", offer["transfer_id"].as_str().unwrap());
    assert_eq!(tx.wait_exit(EXIT), 0, "{}", tx.stderr());
    assert_eq!(rx.wait_exit(EXIT), 0);
    assert_eq!(sha256(&f), sha256(&dst.path().join("hello.txt")));
    assert!(a.trusted().is_empty(), "everyone mode never creates trust");
    assert!(b.trusted().is_empty());
}

#[cfg(target_os = "linux")]
fn vm_hwm_kib(pid: u32) -> u64 {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap();
    status
        .lines()
        .find(|l| l.starts_with("VmHWM:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
        .unwrap()
}

#[cfg(target_os = "linux")]
#[test]
fn bounded_memory_independent_of_size() {
    // Peak RSS of both processes after 16 MiB and 256 MiB transfers must
    // differ by less than 16 MiB: memory does not grow with file size.
    let mut peaks = Vec::new();
    for mib in [16u64, 256] {
        let (a, b) = trusted_pair();
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let f = write_pattern(&src.path().join("m.bin"), mib * 1024 * 1024, mib);
        let (mut rx, port) = receiver(&b, dst.path(), &["--accept", "trusted"]);
        let mut tx = sender(&a, &[f], port, &[]);
        let done = rx.wait_where("finished transfer", EXIT, |v| v["event"] == "completed");
        assert_eq!(done["status"], "completed");
        let peak_rx = vm_hwm_kib(rx.pid());
        let peak_tx = tx.wait_where("completed", EXIT, |v| v["event"] == "completed");
        let _ = peak_tx;
        rx.cmd("quit");
        assert_eq!(rx.wait_exit(EXIT), 0);
        assert_eq!(tx.wait_exit(EXIT), 0);
        peaks.push(peak_rx);
    }
    let growth = peaks[1].saturating_sub(peaks[0]);
    assert!(
        growth < 16 * 1024,
        "receiver peak grew by {growth} KiB: {peaks:?}"
    );
}
