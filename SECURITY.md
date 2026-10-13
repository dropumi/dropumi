# Security policy

Last updated: 2026-10-13

## Project status

Dropumi is **pre-alpha** software.

- The secure transfer protocol ([`docs/protocol/v1.md`](docs/protocol/v1.md)) is a draft. No third party has reviewed it.
- The implementation has not been audited. Neither has the Noise library it uses (`snow`).
- Do not use Dropumi for sensitive data yet.

The design, threat model and accepted residual risks are documented in:

- [`docs/protocol/threat-model.md`](docs/protocol/threat-model.md)
- [`docs/adr/`](docs/adr/), especially ADR-0002 (secure channel), ADR-0003 (identity and key storage), ADR-0004 (pairing and trust) and ADR-0006 (integrity and receiver filesystem safety).

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

- The commit SHA, or the output of `dropumi --version`.
- The operating system, its version and the CPU architecture of each device involved.
- The affected area, for example the handshake, pairing, record layer, CBOR decoding, file finalization, the CLI, CI or a dependency.
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
- CI workflows, the dependency policy (`deny.toml`) and the supply chain.

**Out of scope**

- Attacks that need physical access to an unlocked device, or administrator or root rights on it.
- Social engineering of maintainers or users.
- Network flooding that any host on the same network could do to any service.
- Reports from automated scanners with no demonstrated impact.
- Forks and third-party builds.
- Vulnerabilities in upstream crates, such as `snow`, `blake3` or `mdns-sd`. Please report these to the upstream project. If Dropumi is affected, tell us through the private form.

**Known limitations (documented, not vulnerabilities)**

These are listed in the threat model. A report showing that one of them is worse than documented is welcome.

- The responder's static public key can be probed by anyone who connects.
- mDNS TXT records (device name and an 8-character fingerprint hint) allow devices to be tracked on the local network. `receive --no-mdns` and `pair --listen --no-mdns` do not advertise.
- The identity key is stored in a plaintext file protected only by file permissions (Unix mode checks; on Windows, the ACL of the user profile). Phase 2 plans OS keychain storage.
- An address or host-name target does not pin a key in advance unless `--expect-fingerprint` is given. The receiver's identity is reported after the handshake, and it must still be trusted, or confirmed with the 6-digit code when `--allow-unverified` is used.
- Received files are not marked with the macOS quarantine attribute or the Windows Mark-of-the-Web, so opening a received program may not trigger the Gatekeeper or SmartScreen checks that a download would. Treat received programs with care.
- Protocol v1 has no rekeying, in-protocol key rotation or revocation. `dropumi identity reset --yes` replaces the key, and every peer must pair again.

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

Dropumi stores its identity and trust data in a configuration directory. It is chosen in this order:

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
