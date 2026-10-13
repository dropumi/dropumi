# `dropumi` command-line reference

> Pre-alpha. The protocol and implementation are unaudited. This reference matches the code in `crates/cli/` and the
> `--help` output of the milestone 1 build.

```
dropumi [OPTIONS] <COMMAND>
```

| Command | Purpose |
|---|---|
| [`identity`](#identity) | Show or change this device's identity |
| [`trust`](#trust) | Manage trusted devices |
| [`pair`](#pair) | Pair with another device by comparing a 6-digit code |
| [`receive`](#receive) | Receive files |
| [`send`](#send) | Send files |
| [`discover`](#discover) | List nearby devices (mDNS) |

Contents: [global options](#global-options) · [commands](#identity) · [target syntax](#target-syntax) ·
[peer display](#peer-display) · [interaction and control](#interaction-and---control-stdin) ·
[NDJSON events](#ndjson-events) · [exit codes](#exit-codes) · [files and environment](#files-and-environment) ·
[examples](#worked-examples) · [testing hooks](#testing-hooks)

## Global options

Global options may appear before or after the command.

| Option | Meaning |
|---|---|
| `--config-dir <DIR>` | Configuration directory (identity key and trusted devices). Environment: `DROPUMI_CONFIG_DIR`. |
| `--json` | Write machine-readable events (NDJSON) to stdout. |
| `--control-stdin` | Read control commands (NDJSON) from stdin instead of prompting. |
| `-v`, `--verbose` | More log output on stderr. Repeatable: `-v` info, `-vv` debug, `-vvv` trace. Default: warnings and errors. |
| `-q`, `--quiet` | Only errors on stderr. Cannot be combined with `-v`. |
| `-h`, `--help` | Print help. |
| `-V`, `--version` | Print the version. |

Logs always go to stderr. `DROPUMI_LOG` overrides `-v`/`-q` (see [environment](#environment-variables)).

## `identity`

```
dropumi identity show
dropumi identity set-name <NAME>
dropumi identity reset --yes
```

| Subcommand | Behaviour |
|---|---|
| `show` | Prints the device name, the display fingerprint and the full fingerprint. Creates the identity on first use (random X25519 key, random name such as `Dropumi-3FA9`). |
| `set-name <NAME>` | Changes the device name. The name is sanitized (Unicode NFC, control, separator and format characters removed, at most 64 bytes); an empty result becomes `Unnamed device`. Other devices see this name only as a claim. |
| `reset --yes` | Replaces the identity key and keeps the name. Every paired device must pair again. Without `--yes` it refuses (exit 2). The local trust store is not changed. |

All three subcommands create the identity first if it does not exist. Event: [`identity`](#events-reference).

Fingerprint forms: the **full** form is 52 upper-case base32 characters (A-Z, 2-7); the **display** form is the first
24 characters in groups of four (`XXXX-XXXX-XXXX-XXXX-XXXX-XXXX`); the mDNS **hint** is the first 8 characters.

## `trust`

```
dropumi trust list
dropumi trust add <FINGERPRINT> [--name <ALIAS>]
dropumi trust remove <FINGERPRINT>
```

| Subcommand | Behaviour |
|---|---|
| `list` | Lists trusted devices: display fingerprint, alias, how trust was created (`sas` or `manual`). |
| `add <FINGERPRINT>` | Trusts a device by its **full 52-character fingerprint** (case-insensitive; `-` and spaces ignored). Compare it with the other device out of band first. `--name` sets the local alias. If the device is already trusted, its alias is updated when `--name` is given and everything else is kept. Prints the updated list. |
| `remove <FINGERPRINT>` | Stops trusting a device (full fingerprint). Reports whether it was trusted. |

A shorter prefix is refused (exit 2). There is no trust on first use: trust is created only by `trust add` or by
[`pair`](#pair). Events: `trusted_devices`, `trust_removed`.

## `pair`

```
dropumi pair --listen [--window-minutes <N>] [--bind <IP>] [--port <PORT>] [--no-mdns]
dropumi pair --to <TARGET> [--expect-fingerprint <FP>]
```

Exactly one of `--listen` and `--to` is required.

| Option | Default | Meaning |
|---|---|---|
| `--listen` | | Wait for another device to pair with this one. |
| `--to <TARGET>` | | Device to pair with: address, fingerprint prefix or name ([target syntax](#target-syntax)). |
| `--expect-fingerprint <FP>` | | Require this fingerprint prefix, at least 24 characters. Only with `--to`. |
| `--window-minutes <N>` | `5` | How long `--listen` waits for a pairing attempt. |
| `--bind <IP>` | `0.0.0.0` | Address to listen on (`--listen`). |
| `--port <PORT>` | 47321 | TCP port (`--listen`). Without `--port`, a busy default port falls back to a free port; `0` always picks a free port. |
| `--no-mdns` | | Do not advertise over mDNS (`--listen`). With `--to`, name and fingerprint targets then cannot be resolved. |

How it works:
1. `pair --listen` accepts **one** connection inside the window; the window closes after that attempt, whatever its
   result. If no attempt arrives in time, it fails with `timeout` (phase `sas`, exit 5).
2. Both devices show the same 6-digit code (`sas` event). Each user confirms only if the codes match.
3. Trust is stored on **both** sides only if **both** users confirmed (method `sas`, alias = the other device's
   claimed name at pairing time, which you can change with `trust add <FP> --name`). Otherwise nothing is stored
   (`pairing_rejected`, exit 6).

`pair --to` browses for name and fingerprint targets for 3 s and uses a 3 s connect timeout per address (not
configurable). `pair --listen` refuses transfer sessions, and `receive` refuses pairing sessions (`not_authorized`).

## `receive`

```
dropumi receive [--dir <DIR>] [--policy trusted-only|everyone] [--everyone-minutes <N>]
                [--accept prompt|trusted|never] [--once] [--bind <IP>] [--port <PORT>] [--no-mdns]
```

| Option | Default | Meaning |
|---|---|---|
| `--dir <DIR>` | Downloads folder | Directory for received files. Must already exist; it is never created. Environment: `DROPUMI_RECEIVE_DIR`. |
| `--policy <POLICY>` | `trusted-only` | Who may send. `trusted-only`: only trusted devices. `everyone`: any device, for a limited time; offers from unverified devices always ask and show a code. |
| `--everyone-minutes <N>` | `10` | Duration of `--policy everyone`. Afterwards the policy reverts to `trusted-only` (`policy_changed` event). |
| `--accept <MODE>` | `prompt` | What to do with authorized offers. `prompt`: always ask. `trusted`: accept trusted devices automatically, ask for others. `never`: reject everything without asking. |
| `--once` | | Exit after the first session that delivers an offer, with that transfer's exit code. Sessions that end before an offer (for example a refused unknown sender) do not count. |
| `--bind <IP>` | `0.0.0.0` | Address to listen on. |
| `--port <PORT>` | 47321 | TCP port. Without `--port`, a busy default port falls back to a free port; `0` always picks a free port. Read the actual port from the `listening` event. |
| `--no-mdns` | | Do not advertise over mDNS. |

Behaviour:
- At start, partial files left by a crash (exactly `.dropumi-` + 12 letters or digits + `.part`, older than 15 minutes)
  are removed from the receive directory, with a warning.
- An unverified device is never accepted automatically, whatever `--accept` says.
- Each file is written to a new temporary `.dropumi-*.part` file, checked against its BLAKE3 hash, fsynced and then
  moved to its final name without overwriting anything (`name (1).ext`, `name (2).ext`, ... on collisions; after 256
  tries for one file, or past `(9999)`, a random `name-<12 hex digits>.ext`). Symlinks
  are never followed. On Unix, received files have mode 0600.
- File names are sanitized (`SafeFileName::sanitize`): last path component only; control, bidirectional and invisible
  characters removed; Unicode NFC; `<>:"|?*` replaced with `_`; leading spaces and trailing spaces and dots trimmed;
  Windows device names (`CON`, `NUL`, ...) and the `.dropumi-` prefix get a leading `_`; empty names become `file`;
  at most 255 bytes (and at most 255 UTF-16 code units when decomposed, the HFS+ limit), keeping an extension of up
  to 16 bytes; the device-name and prefix rules are applied again after truncation.
- `receive` runs until Ctrl-C (exit 130) or the `quit` control command (exit 0), unless `--once` is given. The graceful
  shutdown waits at most 4 s (twice the 2 s drain time) for open sessions, plus up to 2 s to withdraw the mDNS
  advertisement and stop the mDNS daemon; a second Ctrl-C exits at once (130); partial files left behind are removed by a
  later `receive` once they are older than 15 minutes.
- When bound to `0.0.0.0`, the CLI does not list the machine's LAN addresses; look them up with your OS tools.

## `send`

```
dropumi send <FILE>... --to <TARGET> [--to <TARGET>...] [--expect-fingerprint <FP>] [--allow-unverified]
                       [--connect-timeout-ms <MS>] [--discover-timeout-ms <MS>]
```

| Option | Default | Meaning |
|---|---|---|
| `<FILE>...` | | Files to send (at least one, at most 10,000). Regular files only; a symlink named on the command line is followed; directories are refused. |
| `--to <TARGET>` | | Receiver: address, `host:port`, fingerprint prefix or device name ([target syntax](#target-syntax)). Repeat it (addresses and host names only, at most 8) to try several addresses in order. |
| `--expect-fingerprint <FP>` | | Require this fingerprint prefix, at least 24 characters. Checked before this device reveals its identity. |
| `--allow-unverified` | | Allow an untrusted receiver after the local user confirms the 6-digit code. Without it, an untrusted receiver is refused (`unverified_peer`, exit 6) before this device reveals its identity. |
| `--connect-timeout-ms <MS>` | `3000` | Connect timeout per address. |
| `--discover-timeout-ms <MS>` | `3000` | How long to browse mDNS for name or fingerprint targets. |

The sender reports success (`completed`, exit 0) only after the receiver's final acknowledgement says every file was
stored, fsynced and visible under its final name. If some files failed, it reports `completed_with_failures` (exit 8).
A source file that changes size or modification time while being sent is aborted (`file_sent` status `aborted`) and
the other files continue.

## `discover`

```
dropumi discover [--timeout-ms <MS>]
```

Browses mDNS (`_dropumi._tcp`, IPv4) for `--timeout-ms` (default `3000`) and prints one line per device seen: name,
fingerprint hint and addresses. Results are reported when the browse window ends. Names and hints are unauthenticated
hints; identity is checked only by the handshake. Exit 0 even if nothing was found. Builds without the `mdns` cargo
feature, or hosts where mDNS cannot start, exit 2.

## Target syntax

`--to` values are classified in this order (`domain::discovery::Target::parse`; leading and trailing spaces ignored):

| # | Form | Example | Result |
|---|---|---|---|
| 1 | Socket address (IPv4, or IPv6 in brackets) | `192.168.1.20:47321`, `[fd00::20]:47321` | Connect to that address. |
| 2 | `host:port` (letters, digits, `-`, `.`) | `laptop.local:47321` | Resolved by the OS resolver; every address becomes a candidate. |
| 3 | Fingerprint prefix: 8 to 52 base32 characters (A-Z, 2-7) after removing `-` and spaces; case-insensitive | `ABCD-EFGH`, `abcdefghijklmnop` | Browse mDNS; the device whose 8-character hint equals the first 8 characters of the prefix. |
| 4 | Anything else: device name, exact match, case-insensitive | `Alice's Mac` | Browse mDNS; the device with that advertised (sanitized) name. |

- Empty targets and port 0 are refused (exit 2).
- Always include the port: an address without a port (`192.168.1.20`) is classified as a device name and fails to
  resolve.
- Name and fingerprint targets need mDNS. They must match exactly one discovered device; no match or several matches
  exit 2 (an ambiguous result lists the candidates).
- **Pitfall:** a name made only of letters, digits 2-7, spaces and hyphens, with 8 to 52 letters and digits (for
  example `Study laptop`), is classified as a fingerprint prefix in step 3, so it cannot be used as a name target. Use the
  address or the fingerprint prefix for such devices, or rename one with `identity set-name`. Default names
  (`Dropumi-XXXX`) always contain a 0, 1, 8 or 9, which are not base32 characters, so they are never affected.

**Identity binding.** Discovery data never proves identity. Before revealing itself (before Noise msg3), the sender
checks the receiver's authenticated key:

| Target | Required of the receiver's key |
|---|---|
| Fingerprint prefix | Starts with the prefix. If the prefix matches exactly one trusted device, it must be that exact key; if it matches several trusted devices, the target is ambiguous (exit 2). |
| Name | Starts with the discovered device's 8-character hint; the exact key if that hint matches exactly one trusted device. |
| Address or host name | No constraint from the target. |
| `--expect-fingerprint` (any target) | Starts with the given prefix (at least 24 characters). |

A mismatch fails with `peer_identity_mismatch` (exit 6). Independently, the receiver must be trusted unless
`--allow-unverified` is given. Discovered addresses are tried in this order: loopback (testing only), private IPv4,
other IPv4, link-local IPv4; discovered IPv6 addresses are not used. Manual addresses come first, in the order given.

## Peer display

Peer names are claims; the identity is the key. Every peer shown to a person uses this rule (`output::peer_line`):

| Peer | Human output |
|---|---|
| Trusted, with a local alias | `Trusted "<alias>" (claims "<claimed name>") · XXXX-XXXX-XXXX-XXXX-XXXX-XXXX` |
| Trusted, no alias | `Trusted device (claims "<claimed name>") · XXXX-...` |
| Not trusted | `Unverified device (claims "<claimed name>") · fp XXXX-...` |

- The alias comes only from the local trust store; the claimed name comes from the peer's Hello.
- When a verification code applies, ` · SAS NNN NNN` is appended.
- Every peer-supplied string shown to a person passes through `terminal_safe`: ANSI escape sequences, control,
  separator, bidirectional and invisible format characters are removed. Emoji joiners and variation selectors stay.
- In NDJSON, the `peer` object has `fingerprint` (full), `fingerprint_display`, `trusted`, `alias` (local, or `null`)
  and `claimed_name` (sanitized, untrusted). Strings are JSON-escaped; consumers must still treat `claimed_name`,
  `files[].name` and `peer_found.name` as untrusted text.

## Interaction and `--control-stdin`

Commands that need a decision (`receive`, `pair`, and `send` with `--allow-unverified`) get it in one of three ways:

| Mode | When | Offers | Verification codes |
|---|---|---|---|
| Control channel | `--control-stdin` | `accept` / `reject` commands | `confirm` / `deny` commands |
| Terminal | stdin is a TTY and no `--control-stdin` | Prompt on stderr; lines typed before the prompt are discarded, and on Unix input still queued in the terminal is flushed before every prompt (on Windows it is not, so a line typed before a prompt can answer it); `y`/`yes` accepts, anything else (or end of input) rejects | Same |
| Never | neither | Rejected (reason `declined`), with a warning | Denied, with a warning |

Nothing is ever accepted or confirmed by default. Unanswered offers are rejected after 120 s (`rejected`, reason
`timeout`); unanswered codes are denied after 120 s.

**Control commands** are JSON objects, one per line, on stdin:

| Line | Effect |
|---|---|
| `{"cmd":"accept"}`, `{"cmd":"accept","transfer_id":"<hex>"}` | Accept the waiting offer: the one whose `transfer_id` (from `offer_received`, compared case-insensitively) is given, or the only waiting offer. |
| `{"cmd":"reject"}`, `{"cmd":"reject","transfer_id":"<hex>"}` | Reject the waiting offer, chosen the same way (reason `declined`). |
| `{"cmd":"confirm"}` | Confirm the verification code that is waiting. |
| `{"cmd":"deny"}` | Deny the verification code that is waiting. |
| `{"cmd":"cancel"}` | `send`: cancel the transfer (exit 4). `receive`: cancel the transfers running now (`Cancel USER` to each peer) and keep listening. `pair`: cancel pairing (`cancelled` event, exit 4, nothing stored). |
| `{"cmd":"quit"}` | Stop the command. `receive` stops listening, cancels running transfers with `Cancel SHUTDOWN` and exits 0. `send` cancels (exit 4). `pair` is cancelled (exit 4). |

- A decision answers only a prompt that is waiting when the decision arrives; it is never queued for a later prompt.
  Send it after the matching `offer_received` or `sas` event. Without `transfer_id`, an offer decision needs exactly
  one waiting offer. A decision that matches no waiting prompt, or several, is dropped with a `warning` event.
- Extra keys are ignored. Unknown commands, invalid JSON and a `transfer_id` that is not a string produce a `warning`
  event. End of input is ignored: closing stdin never stops a receiver.
- `identity`, `trust` and `discover` do not read stdin.

## NDJSON events

With `--json`, every stdout line is one JSON object:

```
{"v":1,"t_ms":<ms since start>,"event":"<name>", ...event fields}
```

- `v` is the event schema version (1). `t_ms` counts milliseconds since the process started its output.
- Human output is suppressed on stdout. Logs, prompts and some notices (verification codes, failures) still go to
  stderr. The human progress line is drawn on stderr only when stderr is a terminal and `--json` is off.
- `transfer_id` is 32 lowercase hex characters. Fingerprints are the 52-character full form unless the field ends in
  `_display`.
- Some early errors (invalid command-line arguments, which clap reports with exit 2; configuration directory cannot be
  determined; receive directory missing; listener cannot bind) are reported on stderr only, with an exit code but no
  `failed` event.

### Events reference

| Event | Emitted by | Fields |
|---|---|---|
| `identity` | `identity *` | `fingerprint`, `fingerprint_display`, `public_key` (64 hex), `name` |
| `trusted_devices` | `trust list`, `trust add` | `devices`: array of {`fingerprint`, `fingerprint_display`, `alias`, `method` (`sas`/`manual`), `added_at`, `last_seen`} (Unix seconds; `last_seen` may be `null`) |
| `trust_removed` | `trust remove` | `fingerprint`, `existed` |
| `listening` | `receive`, `pair --listen` | `addrs` (bound socket addresses), `port`, `fingerprint`, `fingerprint_display`, `name`, `policy` (`trusted-only`/`everyone`; `pair --listen` always reports `trusted-only`) |
| `advertising` | `receive`, `pair --listen` | `instance` (random mDNS instance label) |
| `discovery_unavailable` | `receive`, `pair --listen` | `reason`. Manual addresses still work. |
| `peer_found` | `discover` | `instance`, `name`, `fp_hint` (or `null`), `addrs`, `port`, `versions` |
| `connecting` | `send`, `pair --to` | `addr` (one per candidate tried) |
| `connected` | all session commands | `peer`, `handshake_us` (preamble to end of the Hello exchange), `handshake_ms` (`handshake_us / 1000`, rounded down) |
| `sas` | `pair`, `send`, `receive` (once per authenticated session; `receive` emits it before the offer) | `code` (6 digits as a string), `peer`. Human output: `Session code: NNN NNN` on stdout for a trusted peer, a `Verification code` notice on stderr for an unverified one. A confirmation is asked only when pairing and for `send --allow-unverified` to an untrusted receiver; `receive` shows the code again in the prompt for an unverified sender. |
| `offer_sent` | `send` | `transfer_id`, `peer`, `files` (count), `total_bytes` |
| `offer_received` | `receive` | `transfer_id`, `peer`, `sas` (6-digit string for unverified senders, else `null`), `files`: array of {`index`, `name` (sender's wire name, untrusted), `size`}, `total_bytes` |
| `accepted` | `send`, `receive` | `transfer_id` |
| `progress` | `send`, `receive` | `transfer_id`, `bytes_done`, `bytes_total`, `files_done`, `files_total`. At most every 250 ms, plus one at the start and one at the end. |
| `file_sent` | `send` | `transfer_id`, `index`, `status` (receiver's result: `stored`, `integrity_failed`, `write_failed`, `aborted`) |
| `file_saved` | `receive` | `transfer_id`, `index`, `name` (wire name, untrusted), `saved_as` (final sanitized file name, never a path), `size`, `blake3` (hex) |
| `file_failed` | `receive` | `transfer_id`, `index`, `status` (`integrity_failed` or `aborted`) |
| `paired` | `pair` | `peer` (now trusted, alias set) |
| `session_closed` | `receive`, `pair --listen` | `peer` (or `null` before authentication), `code`: a [failure code](#failure-codes) or `cancelled`. A session ended without a transfer outcome (refused, failed handshake, cancelled before an offer). |
| `policy_changed` | `receive` | `policy` (Everyone mode expired) |
| `completed` | `send`, `receive` | `transfer_id`, `status` (`completed` or `completed_with_failures`), `files_stored`, `failed`, `bytes`, `elapsed_ms` |
| `rejected` | `send`, `receive` | `transfer_id`, `reason` |
| `cancelled` | `send`, `receive`, `pair` | `transfer_id` (or `null`), `by` (`local`/`peer`), `reason`. A locally cancelled pairing reports `transfer_id` `null`, `by` `local`, `reason` `user`. |
| `failed` | any | `transfer_id` (or `null`), `code`, `phase` (or `null`), `message`. Usage errors have only `code` = `usage` and `message`. |
| `warning` | any | `message` (for example removed stale partial files, an unknown control command, or an offer rejected for lack of a terminal) |

`message` fields are local, human-readable text and may contain local paths. They are never sent to the peer.

### Value sets

| Field | Values |
|---|---|
| `rejected.reason` | `declined`, `timeout`, `not_authorized`, `insufficient_space`, `unsupported`, `busy`, `unknown` |
| `cancelled.reason` | `user`, `shutdown`, `storage`, `source`, `timeout`, `unknown` |
| `failed.phase` | `connect`, `pre_auth`, `handshake`, `offer`, `consent`, `sas`, `idle`, `write`, `complete_ack`, `drain` (only for `timeout`) |

### Failure codes

`failed.code` is `TransferError::code()` (`crates/domain/src/error.rs`), plus `usage` from the CLI:

| Code | Meaning | Exit |
|---|---|---|
| `usage` | Invalid input, target or combination of options | 2 |
| `incompatible_version` | No common protocol version | 7 |
| `handshake_failed` | Secure handshake failed (tampering, wrong key, bad SAS reveal) | 6 |
| `peer_identity_mismatch` | The receiver's key does not match the expected fingerprint | 6 |
| `unverified_peer` | The receiver is not trusted and `--allow-unverified` was not given | 6 |
| `not_authorized` | The other device refused this device | 6 |
| `pairing_rejected` | Pairing was not confirmed by both users | 6 |
| `sas_rejected` | The local user did not confirm the verification code | 6 |
| `protocol_violation` | The peer broke the protocol | 10 |
| `limit_exceeded` | The peer exceeded a limit | 10 |
| `remote_error` | The peer reported an error | 10 |
| `integrity` | A record failed to decrypt or content failed its check | 8 |
| `insufficient_space` | Not enough storage space | 9 |
| `storage` | Local storage or configuration failure (including insecure permissions) | 9 |
| `source_changed` | A source file changed while sending | 9 |
| `connection_lost` | The connection closed, was refused or became unreachable | 5 |
| `timeout` | A wait expired (see `phase`) | 5 |
| `busy` | The receiver is busy | 5 |
| `internal` | Unexpected local failure | 1 |

## Exit codes

From `crates/cli/src/exit.rs`:

| Code | Meaning |
|---|---|
| 0 | Success. `receive` also exits 0 when stopped with `quit`. |
| 1 | Internal error |
| 2 | Usage: invalid arguments (including clap parse errors), invalid or unresolvable target, mDNS not available, `identity reset` without `--yes`, unreadable or non-regular source files |
| 3 | Rejected by the receiver |
| 4 | Cancelled (by either side), including a cancelled pairing |
| 5 | Network: connection lost or refused, timeout, busy; also a listener that cannot bind |
| 6 | Authentication or trust failure |
| 7 | Incompatible protocol version |
| 8 | Integrity failure, or completed with failures |
| 9 | Local storage or configuration, including insecure permissions and a missing receive directory |
| 10 | The peer violated the protocol or reported an error |
| 130 | Interrupted (Ctrl-C) |

`receive --once` exits with the code of its transfer outcome; `send` always does.

## Files and environment

### Configuration directory

Chosen in this order: `--config-dir`, then `DROPUMI_CONFIG_DIR`, then the OS local configuration directory plus
`dropumi`:

| OS | Default |
|---|---|
| Linux | `$XDG_CONFIG_HOME/dropumi` or `$HOME/.config/dropumi` |
| macOS | `$HOME/Library/Application Support/dropumi` |
| Windows | `%LOCALAPPDATA%\dropumi` (local, so the key never roams) |

| File | Content | Mode (Unix) |
|---|---|---|
| `identity.key` | `DRPMKEY1` + 32-byte X25519 secret key (plaintext file) | 0600 |
| `device.json` | `{"version":1,"name":"..."}` | 0600 |
| `trusted_devices.json` | `{"version":1,"devices":[...]}` | 0600 |
| `.lock` | Advisory lock for read-modify-write | 0600 |

A missing directory is created with mode 0700 (Unix). Writes are atomic (temp file, fsync, rename, then a directory
fsync on Unix).

**Permission checks (Unix, exit 9).** The directory, `identity.key`, `device.json` and `trusted_devices.json` must be
owned by the current user and must not be symbolic links (`.lock` is not checked). The directory, `device.json` and
`trusted_devices.json` must not be writable by group or others; `identity.key` must not be accessible by group or
others at all. The error names the fix (`chmod 700 '<dir>'` or
`chmod 600 '<file>'`). On Windows, ACLs are not inspected; a `warning` event is shown when the directory is outside
`%LOCALAPPDATA%`.

### Receive directory

`--dir`, else `DROPUMI_RECEIVE_DIR`, else the OS Downloads folder if it exists (`XDG_DOWNLOAD_DIR` on Linux,
`$HOME/Downloads` on macOS, the Downloads known folder on Windows), else `$HOME/Downloads` if it exists. The directory
is never created. If none exists, `receive` exits 9.

### Environment variables

| Variable | Effect |
|---|---|
| `DROPUMI_CONFIG_DIR` | Same as `--config-dir`. |
| `DROPUMI_RECEIVE_DIR` | Same as `receive --dir`. |
| `DROPUMI_LOG` | Log filter for stderr in `tracing-subscriber` `Targets` syntax, for example `debug` or `dropumi_application=debug`. When set and valid, it replaces the `-v`/`-q` level. |

Tests and benchmarks always use temporary configuration directories (`mktemp -d`, which creates them with mode 0700),
never the real one.

## Worked examples

Placeholders: `<FP52>` is a full fingerprint, `NNN NNN` a verification code, `192.168.1.20` the receiver's LAN address.

**Identity.**

```sh
dropumi identity show
dropumi identity set-name "Desk PC 1"
dropumi --json identity show        # one "identity" event
```

Human output of `identity show`:

```
Name:        Desk PC 1
Fingerprint: XXXX-XXXX-XXXX-XXXX-XXXX-XXXX
Full:        <FP52>
```

**Manual trust.** Read the other device's full fingerprint from its `identity show`, compare it over a channel you
trust, then:

```sh
dropumi trust add <FP52> --name "Bob's phone"
dropumi trust list
dropumi trust remove <FP52>
```

**Pairing.**

```sh
# Device B: wait up to 5 minutes for one attempt
dropumi pair --listen

# Device A
dropumi pair --to 192.168.1.20:47321
```

Both terminals show `Verification code: NNN NNN` and ask `Does the other device show NNN NNN? [y/N]`. Answer `y` on
both only if the codes match; both then print `Paired: Trusted "<name>" ...`. Scripted, use `--json --control-stdin`
and write `{"cmd":"confirm"}` after the `sas` event.

**Receiving.**

```sh
dropumi receive --dir ./incoming --accept trusted     # paired devices only, no prompt for them
dropumi receive --dir ./incoming --policy everyone    # 10 minutes, every unverified offer asks and shows a code
dropumi --json --control-stdin receive --dir ./incoming --port 0 --no-mdns --once
```

In the last form, wait for `listening` (actual `port`), then answer each `offer_received` with `{"cmd":"accept"}` or
`{"cmd":"reject"}`.

**Sending.**

```sh
dropumi send report.pdf photo.jpg --to 192.168.1.20:47321
dropumi send report.pdf --to ABCD-EFGH                          # fingerprint prefix, found over mDNS
dropumi send report.pdf --to "Alice's Mac"                      # device name, found over mDNS
dropumi send report.pdf --to 192.168.1.20:47321 --expect-fingerprint XXXX-XXXX-XXXX-XXXX-XXXX-XXXX
dropumi send notes.txt --to 192.168.1.20:47321 --allow-unverified   # receiver in Everyone mode; compare the code
dropumi send big.iso --to 10.0.0.5:47321 --to 192.168.1.20:47321    # try two addresses in order
```

**Discovery.**

```sh
dropumi discover --timeout-ms 5000
dropumi --json discover             # one "peer_found" event per device
```

## Testing hooks

These options are hidden from `--help`. They exist for the automated tests and the benchmark script, are not a
stable interface and may change or disappear.

| Option | Accepted by | Effect (default it overrides) |
|---|---|---|
| `--handshake-timeout-ms <MS>` | `receive`, `pair`, `send` | Preamble, handshake and Hellos (10 s) |
| `--idle-timeout-ms <MS>` | `receive`, `pair`, `send` | No data from the peer during a transfer (30 s) |
| `--consent-timeout-ms <MS>` | `receive`, `pair`, `send` | Receiving user's decision (120 s); also sets the sender's decision wait and the receiver's wait for an unverified offer to this value plus 30 s (150 s) |
| `--pair-timeout-ms <MS>` | `receive`, `pair`, `send` | SAS confirmation and pairing result (120 s) |
| `--write-stall-timeout-ms <MS>` | `receive`, `pair`, `send` | A write making no progress (120 s) |
| `--mdns-loopback` | `receive`, `pair`, `send`, `discover` | Run mDNS on loopback interfaces and accept discovered loopback addresses (one-machine tests) |
| `--throttle-bps <N>` | `send` | Limit the send rate to N bytes per second (used to measure cancel latency) |
