# ADR-0004: Pairing, trust and transfer policy

- **Status:** Accepted
- **Date:** 2026-10-13 (reconciled with the milestone 1 code on 2026-10-13)
- **Related:** ADR-0002, ADR-0003, ADR-0005, ADR-0007, [`../protocol/threat-model.md`](../protocol/threat-model.md)

## Context

- The specification requires a secure first-time pairing flow (QR codes or short authentication strings), protection against man-in-the-middle attacks, unauthorized transfers and device spoofing, and says an unauthenticated advertisement or device name is never proof of identity.
- A short authentication string (SAS) taken straight from the Noise handshake hash is not enough. A man-in-the-middle runs two handshakes and chooses some inputs after seeing the honest side's inputs, so it can grind cheap inputs until both 6-digit codes match (about 10^6 attempts). A hash commitment fixes this; it is the construction used by ZRTP and Bluetooth numeric comparison.
- AirDrop's "Everyone for 10 Minutes" is the familiar UX for receiving from a stranger.
- Name- and address-based targeting is risky even between trusted devices: trusted device B could rename itself "Alpha" or answer at an address the user associates with trusted device A.

## Decision

**1. Committed SAS in every XX handshake** (3 × 32 extra bytes per session):
- msg2 payload `{1: C_R}`, where `C_R = SHA-256("dropumi/v1/sas-commit" ‖ N_R)` and `N_R` is 32 random bytes.
- msg3 payload `{1: N_I}`, 32 random bytes.
- The responder's `Hello` (its first transport message) reveals `N_R` in key 7 (`sas_reveal`). The initiator checks it against `C_R`; on a mismatch it sends `Error PROTOCOL_VIOLATION` and fails with `handshake_failed`.
- `SAS = u64be(SHA-256("dropumi/v1/sas" ‖ h ‖ N_I ‖ N_R)[0..8]) mod 10^6`, zero-padded to 6 digits and shown as `123 456`. `h` is the Noise handshake hash.
- Each side's last free choice is made before it can see the other side's nonce, so an attacker gets one guess per attempt: success probability 10^-6.
- No flag auto-confirms a SAS. Without `--control-stdin` and without a terminal, the CLI denies every SAS prompt and rejects every offer.

**2. Pairing methods.**
- **SAS pairing:** `pair --listen [--window-minutes 5]` on one device and `pair --to <TARGET> [--expect-fingerprint FP]` on the other. Both show the SAS. Each side sends `PairResult{confirmed}` after its local user answers (at most 120 s) and then reads the peer's `PairResult` (at most 120 s). Trust is stored on each side only if both confirmed (method `sas`). The stored alias is the sanitized `Hello` name the user saw next to the SAS.
- `pair --listen` accepts a single connection and then closes the window, whatever the outcome, so retries are always human-initiated. If nobody connects within the window it fails with a timeout. It denies TRANSFER intents with `NOT_AUTHORIZED`.
- `receive` answers PAIR intents with `NOT_AUTHORIZED`.
- **Manual trust:** `trust add <52-character fingerprint> [--name ALIAS]` after comparing full fingerprints out of band (method `manual`, no public key stored).
- **No TOFU:** a key seen for the first time is never trusted automatically.
- **QR invite is deferred.** Hello key 8 (`pairing_token`) is reserved for it.

**3. Trust store.** `TrustedDevice{fingerprint, public_key: Option, alias: Option, method: sas | manual, added_at, last_seen: Option}` in `trusted_devices.json` (ADR-0003). Times are Unix seconds.

**4. Transfer policy.** `TransferPolicy { mode: Off | TrustedOnly | Everyone{until}, accept: Prompt | AutoTrusted | Never }`. The default is **TrustedOnly** with **Prompt**. CLI: `receive --policy trusted-only|everyone` and `--accept prompt|trusted|never`.

`domain::policy::authorize` (the receiver's decision after the initiator's `Hello`):

| Intent / mode | Trusted peer | Unverified peer |
|---|---|---|
| PAIR, pairing window open | Allow (verified) | Allow (unverified) |
| PAIR, no window (any `receive`) | Deny | Deny |
| TRANSFER, `Off` | Deny | Deny |
| TRANSFER, `TrustedOnly` | Allow (verified) | Deny |
| TRANSFER, `Everyone` before `until` | Allow (verified) | Allow (unverified) |

- `consent`: `Never` → auto-reject; `AutoTrusted` and verified → auto-accept; everything else → ask the user.
- **Invariant: an unverified peer never yields auto-accept, on any path.** Covered by the exhaustive tests `authorize_matrix_exhaustive` and `unverified_never_auto_accepted`.
- A denied peer receives `Error NOT_AUTHORIZED` right after its `Hello`, without any prompt. The CLI does not expose mode `Off`; it exists for future receivers.
- **Everyone mode:** `receive --policy everyone [--everyone-minutes 10]`. When the time is up the receiver reverts to TrustedOnly and emits a `policy_changed` event. Offers from unverified senders always prompt, showing "Unverified device", the display fingerprint and the SAS to compare with the sender's screen. Accepting a transfer never creates trust.

**5. Sender-side rules (the initiator protects its own metadata).**
- After msg2 and **before msg3**, the initiator verifies the responder's static key:
  - if it does not match the target expectation (below): `peer_identity_mismatch`;
  - if the responder is unverified, the intent is TRANSFER and `--allow-unverified` was not given: `unverified_peer`.
- Either failure closes the connection without sending msg3, so the sender's static key, `Hello`, device name and file names never reach that peer. Both failures exit with code 6.
- With `--allow-unverified`, the sender must confirm the SAS locally (at most 120 s) before the `Offer` is sent; a denial sends `Cancel USER` and fails with `sas_rejected`.
- `pair --to` always proceeds past msg3 for an unverified responder (that is the point of pairing) but still enforces the target expectation.

**6. Target binding** (`domain::connection::expectation_for`). A send or pair target produces a `PeerExpectation { exact: Option<Fingerprint>, prefixes: Vec<FingerprintPrefix> }`:

| Target | Expectation |
|---|---|
| Fingerprint prefix `p` (8 to 52 base32 characters) | Prefix `p`, plus the full fingerprint if `p` matches exactly one trust entry. If `p` matches two or more trust entries: ambiguous, exit 2. |
| Device name | The resolved peer's mDNS `fp` hint as a prefix, plus the full fingerprint if that hint matches exactly one trust entry. With no valid hint there is no constraint (unverified peers are still refused by policy). |
| Address or `host:port` | No constraint: the peer is reported, not pinned. `send` accepts up to 8 address targets and tries them in order. |
| `--expect-fingerprint FP` | Always added as a prefix (at least 24 characters). |

Target parsing order: socket address, `host:port`, fingerprint prefix, otherwise device name. A name made only of base32 characters (8 to 52 after removing `-` and spaces) is therefore parsed as a fingerprint prefix.

**7. Display rule.** Every displayed peer shows the local trust alias first, taken from the trust store and never from the peer's `Hello`, and labels the `Hello` name as a claim:
- trusted: `Trusted "Beta" (claims "Alpha") · ABCD-EFGH-…` (or `Trusted device (claims "…")` without an alias);
- unverified: `Unverified device (claims "X") · fp … · SAS 123 456`;
- NDJSON uses the same object everywhere: `peer: {fingerprint, fingerprint_display, trusted, alias|null, claimed_name}`.

## Implementation

- Code: [`crates/protocol/src/sas.rs`](../../crates/protocol/src/sas.rs), [`crates/domain/src/policy.rs`](../../crates/domain/src/policy.rs), [`crates/domain/src/connection.rs`](../../crates/domain/src/connection.rs) (`PeerExpectation`, `expectation_for`), [`crates/domain/src/discovery.rs`](../../crates/domain/src/discovery.rs) (`Target::parse`), [`crates/application/src/channel/establish.rs`](../../crates/application/src/channel/establish.rs) (check before msg3), [`crates/application/src/usecases/pair.rs`](../../crates/application/src/usecases/pair.rs), [`receive.rs`](../../crates/application/src/usecases/receive.rs), [`send.rs`](../../crates/application/src/usecases/send.rs), [`trust.rs`](../../crates/application/src/usecases/trust.rs), [`crates/cli/src/output.rs`](../../crates/cli/src/output.rs) (display).
- Tests (all passed in the local run on macOS 27 arm64; CI pending): `sas_kat`, `commitment_kat`, `sas_reveal_mismatch_detected`, `mitm_split_handshakes_sas_independent`, `authorize_matrix_exhaustive`, `unverified_never_auto_accepted`, `expectation_rules`, `pin_mismatch_aborts_before_msg3`, `sender_aborts_before_msg3_for_unverified`, `fp_prefix_unique_trust_pins_full_key`, `name_target_binds_fp_hint`, `both_confirm_store_trust`, `one_denies_stores_nothing`, `window_closes_without_attempt`, `everyone_mode_expires`, `pair_intent_denied_in_receive`, `unverified_offer_always_prompts`, `human_peer_line_alias_first_claims_label`, and the e2e tests `pairing::sas_both_confirm_mutual_trust_then_trusted_transfer`, `pairing::sas_deny_stores_nothing`, `policy::*` and `security::mitm_relay_rejected_by_pinned_trust`.

## Consequences

### Positive

- Secure by default: unknown devices cannot push files unless the receiver opens Everyone mode, and even then the user is asked.
- An active attacker is limited to a 10^-6 chance per human-initiated pairing attempt.
- A trusted device cannot impersonate another trusted device by name, mDNS record or fingerprint prefix; address targets always show who actually answered.
- The sender's identity and file names stay confidential towards unverified or unexpected receivers.

### Negative

- SAS security depends on users actually comparing the codes; in Everyone mode a man-in-the-middle succeeds if they skip it.
- A peer reached by IP address or host name is reported, not pinned.
- A device name that looks like a base32 fingerprint prefix cannot be used as a name target; use the fingerprint or an address instead.
- No QR pairing yet; manual trust needs the 52-character fingerprint.
- Everyone-mode prompts are only bounded by the session admission limits (ADR-0005); dedicated prompt rate limiting is deferred.
- No revocation or rotation; removing trust is manual on each device.

## Alternatives considered

- **SAS from the raw handshake hash:** a man-in-the-middle can grind a 6-digit code.
- **Longer SAS without commitment:** would need roughly 64-80 bits, which is no easier than comparing fingerprints.
- **TOFU:** trusts a spoofed key on first sight.
- **QR invite in Phase 1:** a CLI has no natural scanning flow; deferred to the app phases with Hello key 8 reserved.
- **Pre-shared-key pairing (XXpsk3):** the key must be exchanged before the handshake.
- **Auto-accepting unverified transfers:** violates secure-by-default; never allowed.
- **Everyone mode that creates trust:** would turn one accepted file into permanent trust.
- **Pairing through `receive --pairing`:** a dedicated one-shot `pair --listen` keeps receive policy simple.
