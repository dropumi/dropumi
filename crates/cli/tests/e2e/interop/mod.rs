// SPDX-License-Identifier: MPL-2.0
//! The apps' real cores against the real CLI (D11). Opt-in, one module per
//! platform family.

#[cfg(unix)]
mod android;
#[cfg(target_os = "macos")]
mod apple;

/// The host name an advertisement points at (its SRV target), read with
/// `dns-sd -L`, which prints "… can be reached at HOST:PORT …". macOS
/// hosts only.
#[cfg(target_os = "macos")]
fn srv_target(instance: &str) -> (String, u16) {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::sync::mpsc::channel;

    let mut child = Command::new("dns-sd")
        .args(["-L", instance, "_dropumi._tcp", "local."])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("run dns-sd");
    let out = child.stdout.take().expect("stdout");
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines() {
            let Ok(line) = line else { break };
            if let Some(at) = line.find("can be reached at ") {
                let rest = &line[at + 18..];
                let target = rest.split_whitespace().next().unwrap_or("").to_owned();
                let _ = tx.send(target);
                break;
            }
        }
    });
    let target = rx.recv_timeout(crate::support::STEP);
    let _ = child.kill();
    let _ = child.wait();
    let target = target.expect("dns-sd resolved the advertisement");
    let (host, port) = target.rsplit_once(':').expect("HOST:PORT");
    (host.to_owned(), port.parse().expect("port"))
}
