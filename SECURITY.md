# Security policy

Last updated: 2026-10-15

## Project status

Dropumi is **pre-alpha** software.

- The secure transfer protocol ([`docs/protocol/v1.md`](docs/protocol/v1.md)) is a draft. No third party has reviewed it.
- The implementation has not been audited. Neither has the Noise library it uses (`snow`).
- Do not use Dropumi for sensitive data yet.

The design, threat model and accepted residual risks are documented in:

- [`docs/protocol/threat-model.md`](docs/protocol/threat-model.md)
- [`docs/adr/`](docs/adr/), especially ADR-0002 (secure channel), ADR-0003 (identity and key storage), ADR-0004 (pairing and trust), ADR-0006 (integrity and receiver filesystem safety), ADR-0009 (the FFI facade for the apps), ADR-0012 (the Apple app) and ADR-0014 (key storage in the apps).

## Supported versions

Dropumi has no releases yet. Only the latest commit on `main` receives security fixes.

| Version | Receives security fixes |
|---|---|
| Latest commit on `main` | Yes |
| Any older commit, fork or third-party build | No |

## Reporting a vulnerability

**Report vulnerabilities privately, and only through GitHub private vulnerability reporting:**

<https://github.com/dropumi/dropumi/security/advisories/new>

You can also open the repository's **Security** tab and choose **Report a vulnerability**.

- **Private vulnerability reporting is currently disabled for this repository. The maintainers must enable it** (repository settings, "Private vulnerability reporting"); until then the form above is not available.
- If the form is not available, do not send the details anywhere else. You may start a [GitHub Discussion](https://github.com/dropumi/dropumi/discussions) that says only that you need a private security contact, with no details about the problem or the affected component. The maintainers will then enable the form.
- There is no email address for security reports.
- **Never report a vulnerability in a public issue, pull request, discussion or comment.**

Reports are triaged by the project maintainers, [@fahmialfareza](https://github.com/fahmialfareza) and [@santoadji21](https://github.com/santoadji21).

### What to include

- The commit SHA the build came from. Also give the output of `dropumi --version` for the CLI, or the version shown at the bottom of the app's **Settings** screen for the app. There are no releases yet, so a version number alone does not identify a build.
- The operating system, its version and the CPU architecture of each device involved.
- The affected area, for example the handshake, pairing, record layer, CBOR decoding, file finalization, the CLI, the app facade (`dropumi-ffi`), the Apple app, its Share Extension or its Bonjour adapter, the Kotlin bindings, CI or a dependency.
- Minimal steps to reproduce. Use synthetic test files and throwaway identities created in temporary configuration directories, for example `dropumi --config-dir "$(mktemp -d)" identity show`.
- The impact you expect, and a suggested fix if you have one.

### What never to send

- Personal files, or any file you actually transferred.
- Identity keys (`identity.key`), trust stores (`trusted_devices.json`) or device files (`device.json`) from a real installation.
- Fingerprints, pairing codes (SAS) or device names of your real devices.
- Full file-system paths that contain your user name, host names, or IP or MAC addresses of your real network.
- Packet captures of your real network.

If a proof of concept needs any of these, generate throwaway values in a temporary configuration directory.

## What happens after you report

| Step | Target |
|---|---|
| Acknowledgement of your report | Within 7 days |
| Initial triage: confirmed or not, severity, affected commits | Within 14 days |
| Coordinated public disclosure | 90 days after the report |

- These targets come from a small volunteer team. We will tell you if we expect to miss one.
- We may agree with you on an earlier date, for example when a fix is ready. We may also agree on a later date if a fix needs more time.
- Fixes land on `main` through a normal pull request. The advisory is published as a GitHub Security Advisory. Where appropriate, a CVE is requested through GitHub.
- Reporters are credited in the advisory unless they ask to stay anonymous.

## Scope

**In scope**

- Protocol v1: the version preamble and prologue binding, the Noise XX handshake, the record layer (nonces, replay, tampering) and the CBOR message codec.
- Authentication, pairing (committed SAS), the trust store, the transfer policy, and the peer-expectation check for name and fingerprint targets.
- File integrity (BLAKE3) and the completion semantics (`FileAck STORED`, `CompleteAck`).
- Receiver filesystem safety: path traversal, overwriting existing files, symlinks, reserved or invisible-character file names, and temporary `.dropumi-*.part` files.
- Resource exhaustion from a peer on the local network: admission limits, size and count limits, and malformed or oversized input.
- Information leaks: local paths or error text sent to a peer, secrets in logs, terminal-escape or bidi injection in output.
- Local key and trust storage, including the permission checks described below.
- mDNS discovery: spoofed records and TXT parsing.
- The `dropumi` CLI.
- The app facade `dropumi-ffi` (`crates/ffi`, UniFFI, ADR-0009) and its generated Swift and Kotlin bindings: prompts (answered by prompt ID, never by the facade itself), the event mailbox, the refusal to send Dropumi's own configuration files, and the discovery rules (TXT parsing, IPv4 only, a cap of 256 devices, and pinning a paired device's key when a send goes to a listed device).
- The Apple app for iPhone, iPad and Mac (`apps/apple`, ADR-0012): the consent and code screens (no automatic answers, no trust from an accepted offer, "Looks like" labels, peer strings shown literally), the identity folder (ADR-0014), the receive folders and the Mac menu-bar receiver.
- The app's Share Extension. It never loads the Rust core or uses the network. It copies shared items into an inbox in the App Group `group.io.github.dropumi`, and the app checks each batch against its manifest before it offers the files for sending.
- The app's Bonjour discovery adapter (`apps/apple/DropumiKit/Sources/DropumiCore/BonjourDiscovery.swift`). It registers a proxy with a random host name, so that the system's host name is never the SRV target.
- The Android Kotlin bindings (`apps/android/core-bindings`). There is no Android app yet.
- CI workflows, the dependency policy (`deny.toml`) and the supply chain.

**Out of scope**

- Attacks that need physical access to an unlocked device, or administrator or root rights on it.
- Social engineering of maintainers or users.
- Network flooding that any host on the same network could do to any service.
- Reports from automated scanners with no demonstrated impact.
- Forks and third-party builds.
- Apps that you granted accessibility (UI automation) access to, which can drive Dropumi's screens like a user.
- Vulnerabilities in upstream crates, such as `snow`, `blake3`, `mdns-sd` or `uniffi`. Please report these to the upstream project. If Dropumi is affected, tell us through the private form.

**Known limitations (documented, not vulnerabilities)**

These are listed in the threat model. A report showing that one of them is worse than documented is welcome.

- The responder's static public key can be probed by anyone who connects.
- mDNS TXT records (device name and an 8-character fingerprint hint) allow devices to be tracked on the local network. `receive --no-mdns` and `pair --listen --no-mdns` do not advertise. The apps advertise while they receive, which iPhone and iPad do only while Dropumi is open. To stop advertising, set **Who can send to this device** to **Off** in the app's Settings; on the Mac, **Quit Dropumi completely** in the menu bar also stops it.
- Native advertising may publish the system's host name, which can contain the owner's name. The Apple app's Bonjour adapter registers a proxy with a random host name instead. What an iPhone, iPad or Mac running the app actually publishes is Not tested.
- The identity key is stored in a plaintext file protected only by file permissions.
  - **CLI:** Unix mode checks; on Windows, the ACL of the user profile. Backups can include the configuration directory; on macOS, Time Machine does.
  - **Apple app** (ADR-0014): the same key file, with the same 0700 folder and the same owner and mode checks, in `Application Support/Dropumi` inside the app's own container (never the App Group the Share Extension uses). The app excludes the folder from backups; on iPhone and iPad it also sets the data protection class `completeUntilFirstUserAuthentication`. Any code running as the app's user, and root, can still read the key. The backup exclusion relies on each OS honouring the flag; whether iOS device-to-device migration honours it is UNVERIFIED.
  - Keychain storage (iOS and macOS) and Android Keystore storage are planned for a later milestone, behind the existing `IdentityStore` port.
- An address or host-name target does not pin a key in advance unless `--expect-fingerprint` is given. The receiver's identity is reported after the handshake, and it must still be trusted, or confirmed with the 6-digit code when `--allow-unverified` is used.
- Received files are not marked with the macOS quarantine attribute or the Windows Mark-of-the-Web, so opening a received program may not trigger the Gatekeeper or SmartScreen checks that a download would. Treat received programs with care.
- Another app can draw over a consent prompt (Android overlays), and keys typed ahead can reach a prompt (keyboards on Mac and iPad). The Apple app's prompts have no default button, Escape declines, and Accept and "Codes match" are enabled only 600 ms after the prompt appears. On the Mac, an offer posts a notification without an Accept action and never brings the window forward.
- UniFFI generates `unsafe` code that Dropumi did not write, and `forbid(unsafe_code)` in `dropumi-ffi` does not cover it. UniFFI is pinned to one version (`=0.32.2`), and the committed bindings show every change.
- Events from `dropumi-ffi` carry peer names and file names (sanitized, never paths). An app that logged events would record them; the apps must not log event contents.
- Protocol v1 has no rekeying, in-protocol key rotation or revocation. `dropumi identity reset --yes` (in the app: **Settings**, **This device**, **Reset identity**) replaces the key, and every peer must pair again.

## Safe harbor

We consider security research that follows this policy authorized and done in good faith. We will not start or support legal action against you for it.

Please:

- test only on devices, accounts and networks you own or have permission to use;
- use throwaway identities and synthetic files;
- do not access, change or keep other people's data. If you encounter personal data, stop and tell us;
- do not degrade service for other people on a shared network;
- give us a reasonable time to fix the issue before you disclose it, following the timeline above.

We cannot authorize testing of systems, devices or networks that belong to someone else.

## Protecting your local configuration

This section describes the `dropumi` CLI. The Apple app keeps the same files in a folder inside its own app container and runs the same Unix checks on them (see the known limitations above and ADR-0014).

The CLI stores its identity and trust data in a configuration directory. It is chosen in this order:

1. `--config-dir DIR`
2. the `DROPUMI_CONFIG_DIR` environment variable
3. the OS default (the per-user local configuration directory):
   - Linux: `$XDG_CONFIG_HOME/dropumi`, or `~/.config/dropumi` if that variable is not set;
   - macOS: `~/Library/Application Support/dropumi`;
   - Windows: `%LOCALAPPDATA%\dropumi`.

| File | Contents | Created with (Unix) |
|---|---|---|
| `identity.key` | The device's private X25519 key. Anyone who can read this file can impersonate the device. | `0600` |
| `device.json` | The device name | `0600` |
| `trusted_devices.json` | The trusted devices. Anyone who can write this file can add a trusted device. | `0600` |
| `.lock` | Lock file that serializes changes between processes | `0600` |

If the directory does not exist, Dropumi creates it with mode `0700` on Unix. An existing directory is never modified; it is only checked.

Received files are also created with mode `0600` on Unix, in the receive directory (`--dir`, `DROPUMI_RECEIVE_DIR`, or your Downloads folder).

### Unix (Linux, macOS)

The commands that read the configuration (`identity`, `trust`, `pair`, `receive` and `send`) check it before they use it. `discover` does not read the configuration directory. Dropumi refuses to continue and exits with **code 9** if:

- the directory is not owned by the current effective user, is a symbolic link, or is writable by group or others (`mode & 0o022`);
- `device.json` or `trusted_devices.json` has a different owner, is a symbolic link, or is writable by group or others (`mode & 0o022`);
- `identity.key` has a different owner, is a symbolic link, or is readable, writable or executable by group or others (`mode & 0o077`).

The error message names the file and suggests `chmod 700` (directory) or `chmod 600` (file). To repair the default permissions:

```sh
chmod 700 <config-dir>
chmod 600 <config-dir>/identity.key <config-dir>/device.json <config-dir>/trusted_devices.json
```

### Windows

Dropumi does **not** inspect Windows ACLs, and the owner and mode checks above do not run on Windows. Dropumi relies on the permissions of `%LOCALAPPDATA%`, which is inside your user profile.

If the configuration directory is outside `%LOCALAPPDATA%` (for example through `--config-dir` or `DROPUMI_CONFIG_DIR`), the commands that load the identity (`identity`, `pair`, `receive` and `send`) print a warning and continue. A directory you create elsewhere inherits the access rules of its parent. On a shared folder or a shared drive, other users could read your key or add trusted devices.

Keep the configuration under `%LOCALAPPDATA%`, or check who can access the directory:

```bat
icacls "%LOCALAPPDATA%\dropumi"
```

### General advice

- Never share the contents of the configuration directory in issues, logs or screenshots.
- Use full-disk encryption on devices that hold a Dropumi identity.
- If you think `identity.key` was exposed:
  1. Run `dropumi identity reset --yes` to create a new identity.
  2. Ask your peers to remove the old fingerprint with `dropumi trust remove <full 52-character fingerprint>`.
  3. Pair again.

  Protocol v1 has no revocation.
- For testing and experiments, use a temporary directory (`--config-dir "$(mktemp -d)"`) so that your real identity and trust store are never touched.
