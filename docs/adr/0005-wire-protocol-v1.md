# ADR-0005: Wire protocol v1

- **Status:** Accepted (draft protocol)
- **Date:** 2026-10-13 (reconciled with the milestone 1 code on 2026-10-14)
- **Related:** ADR-0002, ADR-0004, ADR-0006, [`../protocol/v1.md`](../protocol/v1.md) (normative text, CDDL schema and state machines)

## Context

- The specification asks for a versioned, documented, interoperable protocol with version and capability negotiation, authenticated session establishment, transfer request and acceptance, multiple files, streaming with bounded memory, backpressure, cancellation, error recovery, per-file integrity, clear completion semantics, and room for later extensions (resume, folders) without breaking compatibility.
- It must resist malformed metadata, oversized input, excessive allocation, resource exhaustion, replay and version incompatibility.
- The protocol is the interoperability contract between future native clients, so its parser must be strict and deterministic: one valid encoding, one parse path.
- v1 is a draft. It has not been reviewed by a third party and may change before it is declared stable.

## Decision

**Connection layout** (TCP in v1; any reliable, ordered byte stream works):

```
I→R  Preamble       "DRPM" 0x01 n:u8 (1..=8) n × u16be, strictly descending          (≤ 22 B)
R→I  PreambleReply  "DRPM" 0x01 status: 0 OK + selected u16be
                                       | 1 NO_COMMON_VERSION + n + n × u16be, close
                                       | 2 BUSY, close
I→R  u16be 32  ‖ msg1   -> e              payload empty
R→I  u16be len ‖ msg2   <- e, ee, s, es   payload CBOR {1: sas_commit bstr32}
     [initiator verifies the responder; on failure it closes WITHOUT msg3 (ADR-0004)]
I→R  u16be len ‖ msg3   -> s, se          payload CBOR {1: sas_nonce bstr32}
both u16be len (17..=65,535) ‖ Noise transport message, implicit per-direction nonce
```

**Version negotiation and downgrade protection.**
- The responder selects the highest common version; the initiator checks that the selected version is in its own list. A bad magic, format byte, count or order makes the responder close without replying.
- Prologue = `"DROPUMI-PROLOGUE" ‖ u8 len(P) ‖ P ‖ u8 len(R) ‖ R`, using the exact preamble and reply bytes from the wire. Any tampering makes the handshake fail: an attacker can cause a denial of service but never a downgrade.
- Implementations never retry automatically with a lower version (`incompatible_version`, exit 7). v1 means `Noise_XX_25519_ChaChaPoly_SHA256` (ADR-0002).

**Records and frames.**
- Every record is `u16be length ‖ Noise message`. Handshake records are 1..=512 bytes (payload at most 256 bytes; msg1 exactly 32 bytes). Transport records are 17..=65,535 bytes. The length is validated before any allocation; transport buffers are allocated once per session.
- Plaintext byte 0 is the frame type: `0x01` CONTROL carries exactly one CBOR message (body at most 65,518 bytes); `0x02` DATA carries 1..=65,518 file bytes, initiator to responder only, only inside an open file. An empty frame, an unknown frame type or a misplaced DATA frame is a protocol violation.
- **No fragmentation:** one record = one AEAD unit = one complete frame. Large offers are split into an `Offer` followed by `OfferFiles` batches, so no attacker-controlled reassembly buffer exists.

**Encoding: strict CBOR** (RFC 8949) via `minicbor` 2.3.0 (`alloc` only, no derive), with hand-written decoders (`protocol::codec::Reader`).
- A message is `[type uint, {uint => value}]`.
- Accepted: definite-length arrays, maps, byte and text strings; integers that fit in 64 bits; booleans. Rejected: indefinite lengths, tags, floats, `null`, `undefined`, other simple values, invalid UTF-8 and trailing bytes.
- Map keys are unsigned integers in strictly ascending order (so duplicates are rejected); at most 32 map entries; nesting depth at most 4. Declared lengths never size an allocation before they are checked.
- Unknown map keys are skipped by a bounded skipper. Unknown message types 0-127 are fatal (`Error UNSUPPORTED_MESSAGE`); types 128-255 are ignorable, at most 16 in a row. A Hello key that belongs to the other role (`intent` from the responder, `sas_reveal` from the initiator) is an unknown key: skipped, never interpreted. Any message sent in the wrong direction is a protocol violation.

**Message catalogue (types).** 1 Hello, 2 PairResult, 8 Error, 9 Cancel, 16 Offer, 17 OfferFiles, 18 Accept, 19 Reject, 20 FileStart, 21 FileEnd, 22 FileAbort, 23 FileAck, 24 Complete, 25 CompleteAck. Types 128-255 are ignorable; v1 defines none in code (`v1.md` reserves 128 for ProgressHint, not emitted). Keys and per-field limits are in `v1.md`.

| Code table | Values |
|---|---|
| Error | 1 PROTOCOL_VIOLATION, 2 UNSUPPORTED_MESSAGE, 3 LIMIT_EXCEEDED, 4 NOT_AUTHORIZED, 5 PAIRING_FAILED, 6 BUSY, 7 TIMEOUT, 8 INTERNAL |
| Reject | 1 DECLINED, 2 TIMEOUT, 3 NOT_AUTHORIZED (reserved), 4 INSUFFICIENT_SPACE (reserved), 5 UNSUPPORTED, 6 BUSY |
| Cancel | 1 USER, 2 SHUTDOWN, 3 STORAGE, 4 SOURCE, 5 TIMEOUT |
| FileAbort | 1 SOURCE_CHANGED, 2 SOURCE_UNREADABLE |
| FileAck status | 0 STORED, 1 INTEGRITY_FAILED, 2 WRITE_FAILED, 3 ABORTED (semantics in ADR-0006) |
| Hello intent | 1 TRANSFER, 2 PAIR |

**Static `Error.detail`.** Key 2 of `Error`, if present, MUST be the fixed English phrase for its code, exactly as returned by `ErrorCode::wire_detail()`: `protocol violation`, `unsupported message`, `limit exceeded`, `not authorized`, `pairing failed`, `busy`, `timeout`, `internal error` (unknown codes: `internal error`). It MUST NOT contain paths, OS error text, file names or any other local data. A received `detail` is parsed (at most 256 bytes) but the milestone 1 code does not display it: only the code is mapped locally (`NOT_AUTHORIZED` → `not_authorized`, `BUSY` → `busy`, `PAIRING_FAILED` → `pairing_rejected`, any other code → `remote_error`, exit 10). Any future display must treat it as untrusted text and sanitize it.

**Capabilities and extensibility.** Capabilities come from the authenticated `Hello`; the effective feature set is the intersection (v1 defines none, so it is always empty). `hash_algs` must contain 1 (BLAKE3-256). Reserved for later: feature ids 1 resume, 2 folders, 3 multiple transfers per session, 4 receiver-pull; FileEntry keys 5 `relative_path` and 6 `content_hash`; Hello key 8 `pairing_token`; Accept key 2 (subset selection); `FileStart.offset` (must be 0 in v1).

**Session rules.** One transfer per session; the initiator is the sender. The sender reports success only after `CompleteAck` with every file STORED; otherwise the outcome is "completed with failures" (exit 8). An EOF without `Complete`, `CompleteAck`, `Cancel` or `Error` is `connection_lost`, which detects truncation. A local cancel sends `Cancel`, shuts down the write half and drains input for at most 2 s, so the peer reads the `Cancel` before any reset.

**Limits.**

| Item | Limit |
|---|---|
| Files per transfer | 1..=10,000 |
| Offer metadata | Encoded `OfferFiles` bodies at most 4 MiB in total; the sum of file sizes must equal `total_size` (checked arithmetic) |
| Wire file name | 1..=1,024 bytes, a single component (no `/`, `\`, NUL, `.` or `..`) |
| File size, total size | ≤ 2^63−1 |
| Device name | ≤ 128 bytes on the wire, ≤ 64 bytes displayed |
| Software / media type / error detail | ≤ 64 / ≤ 127 / ≤ 256 bytes |
| Features / hash algorithms | ≤ 32 / ≤ 8 entries |
| Admission (receiver) | 16 unauthenticated connections globally and 4 per IP (excess dropped immediately); 4 authenticated sessions (excess get preamble BUSY); 1 active session per fingerprint (excess get `Error BUSY` after the Hello) |

**Timeouts** (`application::config::Timeouts::default()`):

| Phase | Default |
|---|---|
| TCP connect, per candidate | 3 s |
| Preamble, handshake and both Hellos | 10 s |
| Receiver waiting for the whole offer (Offer and all OfferFiles; one absolute deadline) | 30 s (trusted sender); 150 s (unverified sender, covers the sender's SAS confirmation) |
| Receiving user's decision | 120 s (the sender waits up to 150 s) |
| SAS confirmation and peer's PairResult | 120 s |
| Receiver idle during a transfer | 30 s |
| **Write stall** (a pending write with no progress; fails with `Timeout(Write)`) | 120 s |
| Sender waiting for CompleteAck (restarted whenever another file is acknowledged) | 60 s |
| Drain after Cancel or Error | 2 s |

**Replay.** A replayed msg1/msg3 fails because the responder's fresh ephemeral key changes the derived keys; transport nonces are implicit; the two directions use separate keys, so reflected records fail.

## Implementation

- Code: [`crates/protocol/src/`](../../crates/protocol/src/) (`version`, `record`, `frame`, `codec`, `message`, `limits`, `session/{sender,receiver}`), [`crates/domain/src/limits.rs`](../../crates/domain/src/limits.rs), [`crates/application/src/config.rs`](../../crates/application/src/config.rs) (timeouts, admission limits), [`crates/application/src/usecases/admission.rs`](../../crates/application/src/usecases/admission.rs), [`crates/application/src/channel/`](../../crates/application/src/channel/).
- Tests (all passed in the local run on macOS 27 arm64; CI pending): the codec tests `rejects_indefinite`, `rejects_tags_floats_undefined`, `rejects_duplicate_or_unsorted_keys`, `rejects_depth_over_4`, `rejects_trailing_bytes`, `enforces_string_and_array_limits`; the property tests `decode_arbitrary_never_panics`, `mutated_valid_never_panics`, `message_roundtrip`, `random_inputs_never_panic`, `valid_sender_trace_accepted`; `wire_details_are_static_and_pathless`, `wrong_direction_rejected`, `ignorable_type_skipped`, `unknown_required_type_unsupported`; the receiver session tests `receiver_rejects_*`; `preamble_rejects_bad_magic_format_count_order`, `prologue_layout`, `busy_and_incompatible_preamble_replies`, `oversized_length_rejected_preauth`, `admission_limits_global_per_ip_and_session`, `stalled_receiver_times_out_sender`; and the e2e tests `security::malformed_traffic_matrix`, `security::incompatible_version_raw_peer`, `security::preamble_tamper_breaks_handshake`.
- Interoperability with any other implementation: Not tested (none exists).

## Consequences

### Positive

- Self-describing and forward-compatible: new optional keys and ignorable types do not break v1 decoders.
- One deterministic parse path; every record decodes on its own; no allocation is sized by unauthenticated input.
- Bounded memory per session regardless of file size; backpressure is plain TCP flow control. Measured peak RSS on the maintainer's machine for one 256 MiB file over loopback: 4,416 KiB for the sender and 4,400 KiB for the receiver (median of 3 runs, `scripts/bench/loopback.sh`).
- Version downgrade is cryptographically detected; the static `Error.detail` rule keeps local data off the wire.

### Negative

- Draft and unaudited; interoperability with any other implementation is untested.
- Hand-written decoders are more code to review than derived ones.
- No resume, folders or multiple transfers per session in v1; one TCP connection per transfer.
- Batching large offers adds message types compared with a single large message.

## Alternatives considered

- **postcard / bincode:** not self-describing; bincode also has RUSTSEC-2025-0141.
- **ciborium:** stale, and serde makes integer keys awkward.
- **prost (Protocol Buffers):** needs protoc or hand-maintained `.proto` tooling.
- **minicbor-derive:** implicit handling of unknown and duplicate keys.
- **Hand-written TLV:** more custom parsing for no gain over strict CBOR.
- **CONTROL_MORE fragmentation for large offers:** requires an attacker-driven reassembly buffer.
- **Automatic retry with a lower version:** enables downgrade attacks.
