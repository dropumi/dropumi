# ADR-0002: Secure channel: Noise XX over TCP

- **Status:** Accepted
- **Date:** 2026-10-13 (reconciled with the milestone 1 code on 2026-10-14)
- **Related:** ADR-0003, ADR-0004, ADR-0005, [`../protocol/v1.md`](../protocol/v1.md), [`../protocol/threat-model.md`](../protocol/threat-model.md)

## Context

- The specification requires authenticated, encrypted sessions built from established primitives and vetted libraries, with no custom cryptography, and protection against man-in-the-middle, replay and protocol downgrade.
- Two devices meeting for the first time do not know each other's keys, and Dropumi is local-only: there is no certificate authority, account or server to vouch for a key.
- The core must cross-compile for Android and iOS. Pure-Rust cryptography avoids a C crypto toolchain on every target.
- `snow` is the established Rust implementation of the Noise Protocol Framework. Its README states that it has not received a formal audit.
- snow 0.10.0's `std` feature lists `ring/std` without the weak-dependency marker (`ring?/std`; checked in the published `Cargo.toml`), so building with default features compiles `ring` even though the ring resolver is unused. The upstream fix (snow PR #194) is not in a release at the time of writing.
- RUSTSEC-2024-0011 (GHSA-7g9j-g5jg-3vv3, "unauthenticated nonce increment") affected the stateful transport in snow before 0.9.5.

## Decision

**Pattern and suite.** `Noise_XX_25519_ChaChaPoly_SHA256` over TCP (`protocol::limits::NOISE_PARAMS`). Protocol v1 is bound to exactly this name; changing the pattern or any primitive requires a new protocol version (ADR-0005).

**Library and features** (`[workspace.dependencies]`):

```toml
snow = { version = "0.10.0", default-features = false, features = [
  "use-curve25519", "use-chacha20poly1305", "use-sha2", "use-getrandom",
] }
```

- Default features are off, so `ring` is not compiled. Checkpoint result: snow 0.10.0 builds and passes every test without default features, so the planned fallback (enable `std` and accept a compiled `ring`) was **not** needed. `ring` does not appear in `Cargo.lock` at all, and `scripts/ci/check-architecture.sh` (job `architecture`) and the `ring` ban in `deny.toml` keep it out (ADR-0010).
- On every snow upgrade, re-check the feature list; once a release contains PR #194 it can be revisited.
- `getrandom` 0.3 and `sha2` 0.10 are kept on snow's major versions (ADR-0010).

**Handshake** (`crates/protocol/src/handshake.rs`, driven by `crates/application/src/channel/establish.rs`).
- Messages: msg1 `-> e` (exactly 32 bytes, empty payload), msg2 `<- e, ee, s, es`, msg3 `-> s, se`. Handshake records are at most 512 bytes and payloads at most 256 bytes (ADR-0005).
- msg2 and msg3 payloads carry the SAS commitment and nonce (ADR-0004).
- The API is a typestate: `Initiator::start` → `InitiatorAwaitMsg2::read_msg2` → `InitiatorVerify` → `proceed()` → `InitiatorDone`, and `Responder::start` → `read_msg1` → `read_msg3` → `ResponderDone`. `InitiatorVerify` exposes the responder's authenticated static key and fingerprint, and msg3 can only be produced by calling `proceed()`. The caller verifies the responder against the target expectation and the trust policy **before** that call (ADR-0004).
- The SAS nonces are passed in by the caller. Noise ephemeral keys come from the OS generator inside snow (`use-getrandom`).
- The handshake hash `h` is read before switching to transport mode, because it feeds the SAS.
- snow error details never leave the protocol crate; callers see `ProtocolError::Handshake`.

**Transport** (`crates/protocol/src/record.rs`).
- After the handshake, `into_stateless_transport_mode()` produces a `StatelessTransportState`, shared through an `Arc` by two owners:
  - `Sealer` (send half) owns the outgoing `u64` nonce. Nonce `u64::MAX` is reserved by the Noise specification, so sealing at that value fails with `NonceExhausted`; increments use `checked_add`.
  - `Opener` (receive half) owns the incoming nonce, increments it only after a successful decryption, and is **poisoned** by any failure: every later call fails, so one bad record ends the session.
- Nonces are implicit and never sent. TCP delivers in order, so any drop, reorder, replay or bit flip fails authentication and is fatal.
- The application layer splits the stream: `RecordReader` (owns the `Opener`; cancel-safe, keeps partial-read state across calls; idle timeout) and `RecordWriter` (owns the `Sealer`; write-stall timeout, default 120 s). Each preallocates its record buffers once per session.
- Stateless mode was chosen for explicit nonce ownership per direction and for sharing one key state between the reader and writer halves without a lock. It is not a workaround for RUSTSEC-2024-0011: that advisory was fixed in 0.9.5 and does not affect 0.10.0.

**No rekey in v1.** Records are at most 65,535 bytes and the nonce is 64-bit, so exhaustion is not reachable in practice; it is still handled as an error. Rekeying is left to a future protocol version.

**Downgrade protection.** The cleartext version preamble and reply are bound into the Noise prologue (ADR-0005), so tampering causes handshake failure rather than a downgrade.

**Identity hiding is accepted as is.** In XX the initiator's static key is sent encrypted to an authenticated responder, but the responder's static key is revealed in msg2 to any initiator (Noise specification §7.8). Anyone who can reach a listening device can learn its static public key. This is documented as a residual risk.

## Implementation

- Code: [`crates/protocol/src/handshake.rs`](../../crates/protocol/src/handshake.rs), [`record.rs`](../../crates/protocol/src/record.rs), [`limits.rs`](../../crates/protocol/src/limits.rs), [`crates/application/src/channel/`](../../crates/application/src/channel/).
- Tests (all passed in the local run on macOS 27 arm64; CI pending): `xx_happy_path_same_hash_and_sas`, `initiator_learns_responder_before_msg3`, `tampered_msg2_fails`, `tampered_msg3_fails`, `replayed_msg1_msg3_fail_at_new_responder`, `prologue_mismatch_fails_at_initiator_msg2`, `record_bitflip_fails`, `record_reorder_fails`, `record_replay_fails`, `directions_use_independent_keys`, `opener_poisoned_after_failure`, `nonce_exhaustion_errors`, `x25519_rfc7748_vector`, and the e2e tests `security::ciphertext_bitflip_aborts`, `security::recorded_session_replay_rejected`, `security::wire_capture_contains_no_plaintext`.
- `ring` absence: checked by the architecture guard (passed locally) and by `cargo deny --locked check` (passed locally).

## Consequences

### Positive

- Mutual authentication with raw X25519 keys on first contact, without certificates.
- Forward secrecy from fresh ephemeral keys on both sides: a later theft of a static key does not decrypt past sessions.
- Pure-Rust cryptography (curve25519-dalek, RustCrypto ChaCha20-Poly1305 and SHA-256) with no C crypto toolchain.
- Nonce handling is explicit, owned per direction, and tested (exhaustion and poisoning).
- Measured on the maintainer's machine over loopback (`scripts/bench/loopback.sh`, macOS 27, Apple M4 Pro, median of 3 runs): handshake latency from preamble to both Hellos 615 µs; one 256 MiB file at 386.1 MiB/s end to end, including BLAKE3 hashing and disk writes.

### Negative

- snow is unaudited, and Dropumi's protocol v1 has had no third-party review.
- A listening responder's static key, and therefore its fingerprint, can be probed by anyone on the network.
- No rekey; long-lived sessions rely on the 64-bit counter bound.
- Throughput on phones, and on real LANs or Wi-Fi, is not measured; it must be benchmarked before mobile releases.
- TCP only; other transports (QUIC, Wi-Fi Direct, BLE) need their own decisions.
- Depending on an unreleased upstream fix means re-checking the feature edge on every snow upgrade.

## Alternatives considered

- **TLS 1.3 with rustls:** needs an Ed25519 or X.509 identity, a custom certificate verifier and a C-backed crypto provider; more code for no first-contact benefit.
- **QUIC with quinn:** UDP is blocked or isolated on many networks, it still needs rustls, and quinn-proto had several advisories in 2026.
- **Noise IK:** needs the responder's key in advance, and snow has no fallback modifier for first contact.
- **Noise XXpsk3:** the pre-shared key must be known before the handshake.
- **snow ring-accelerated resolver:** adds C and assembly builds for little benefit at LAN speeds.
- **AES-GCM suite:** fast and constant-time only with hardware AES.
- **BLAKE2s as the Noise hash:** fine, but SHA-256 is more universally available.
- **Stateful `TransportState` behind a mutex:** nonce ownership becomes implicit and the halves contend on a lock.
- **One I/O task using `select!` for both directions:** puts both directions' crypto on one task.
- **`risky-raw-split` or hand-built crypto:** amounts to writing our own transport; rejected by the specification.
