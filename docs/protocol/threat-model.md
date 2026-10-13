# Dropumi threat model (protocol v1)

| | |
|---|---|
| Status | **Draft. Unaudited.** |
| Covers | Protocol v1 ([`v1.md`](v1.md)), mDNS discovery ([`discovery.md`](discovery.md)), the reference `dropumi` CLI, its receiver filesystem handling and its local configuration storage |
| Document date | 2026-10-14 (test references reconciled with the milestone 1 code and the test run on the same date) |

> Dropumi is pre-alpha. Nothing in this document has been reviewed by a third party. A mitigation listed here is a
> design intent backed by the named tests; it is not a guarantee. Please report suspected vulnerabilities privately as
> described in [`SECURITY.md`](../../SECURITY.md), never in a public issue.

## Contents

1. [Scope and method](#1-scope-and-method)
2. [System summary and trust boundaries](#2-system-summary-and-trust-boundaries)
3. [Assets](#3-assets)
4. [Adversaries](#4-adversaries)
5. [Out of scope](#5-out-of-scope)
6. [Security goals, mitigations and tests](#6-security-goals-mitigations-and-tests)
7. [Residual risks](#7-residual-risks)
8. [Revision history](#8-revision-history)

## 1. Scope and method

- This model covers milestone 1: two `dropumi` CLI processes transferring files over a local network with protocol v1.
  Desktop and mobile apps, the web companion, BLE, Wi-Fi Direct/Aware and hotspots do not exist yet and are out of
  scope.
- Each security goal in [§6](#6-security-goals-mitigations-and-tests) is tied to a mitigation (with a reference to the
  normative text) and to the automated tests that demonstrate it. A gap is written **not covered (planned)**.
- **Test names** are the exact paths printed by `cargo test`, prefixed with the test target:

  | Written as | Test target | Run one test with |
  |---|---|---|
  | `domain <path>` | Unit tests in `crates/domain/src` | `cargo test -p dropumi-domain --lib <path>` |
  | `domain/filename_props <name>` | Property tests in `crates/domain/tests/` | `cargo test -p dropumi-domain --test filename_props <name>` |
  | `protocol <path>` | Unit tests in `crates/protocol/src` | `cargo test -p dropumi-protocol --lib <path>` |
  | `protocol/<target> <name>` | `crates/protocol/tests/{codec_props,handshake,record,session_props}.rs` | `cargo test -p dropumi-protocol --test <target> <name>` |
  | `application <path>` | Unit tests in `crates/application/src` | `cargo test -p dropumi-application --lib <path>` |
  | `application/<target> <name>` | `crates/application/tests/{channel,failures,pairing,policy,transfer}.rs` (in-memory duplex streams, fakes, paused time) | `cargo test -p dropumi-application --test <target> <name>` |
  | `infra <path>` | Adapter tests in `crates/infra/src` (real file system and TCP) | `cargo test -p dropumi-infra --lib <path>` |
  | `cli <path>` | CLI unit tests in `crates/cli/src` | `cargo test -p dropumi-cli --bin dropumi <path>` |
  | `e2e <path>` | Two real `dropumi` processes over loopback TCP (`crates/cli/tests/e2e`) | `cargo test -p dropumi-cli --test e2e <path>` |

- **Test status.** Every test named in [§6](#6-security-goals-mitigations-and-tests) passed in the local workspace run on
  the maintainer's macOS 27 arm64 machine (Rust 1.99.0): 224 passed, 1 ignored, and in a Debian 12 aarch64 container: 225 passed. The ignored test is the opt-in mDNS
  test `e2e discovery::mdns_discover_and_send_by_fingerprint`, which also passed when run locally with
  `DROPUMI_TEST_MDNS=1`. CI has not run yet, so every GitHub-runner result is "Not tested (pending CI)". All automated
  tests run on loopback; real multi-device LAN behaviour is "Not tested". See [`docs/testing.md`](../testing.md).
- **Platform-only tests:** `e2e transfer::bounded_memory_independent_of_size` compiles only on Linux; it passed in the
  Debian 12 aarch64 container run and is pending CI on `ubuntu-latest`. `infra fs::finalize::tests::transient_access_denied_retried`
  and `infra fs::finalize::tests::windows_persistent_denied_fails_without_placeholder` compile only on Windows; they were
  cross-compiled but have not run, so the behaviour they check counts as not covered until CI runs them.

## 2. System summary and trust boundaries

```
                       TB2: discovery (unauthenticated mDNS multicast)
  +------------------+ <---------------------------------------> +-------------------+
  | Any host on the  |                                           | Receiver          |
  | local network    |                                           | dropumi receive   |
  +------------------+                                           | (responder)       |
                                                                 |                   |
  +------------------+   TB1: network (TCP): preamble,           |                   |
  | Sender           | ----------------------------------------> |                   |
  | dropumi send     |   Noise XX handshake, encrypted records   |                   |
  | (initiator)      |                                           +-------------------+
  +------------------+                                              |             |
           |                                       TB3: peer data   |             |  TB4: OS user
           |  TB4: OS user                                          v             v
           v                                         +---------------+   +-----------------------------+
  +-----------------------------+                    | Receive dir   |   | Config dir (0700)           |
  | Config dir (0700)           |                    | .part temp    |   | identity.key (0600)         |
  | identity.key (0600)         |                    | files, final  |   | device.json (0600)          |
  | device.json (0600)          |                    | files (0600)  |   | trusted_devices.json (0600) |
  | trusted_devices.json (0600) |                    +---------------+   +-----------------------------+
  +-----------------------------+

  TB5: on both devices, the user compares SAS codes or fingerprints and gives consent.
```

| Boundary | Description |
|---|---|
| TB1 Network | Every byte from the network is untrusted. Authentication establishes *which key* a peer holds, not whether to trust it. |
| TB2 Discovery | mDNS records (instance names, TXT `n`, `v`, `fp`) are unauthenticated hints. |
| TB3 Peer data → file system | Data from an authenticated, even trusted, peer is still untrusted input to the local file system and terminal. |
| TB4 OS user | Other local accounts must not read the identity key or modify the trust store. |
| TB5 Human | Pairing and Everyone-mode transfers depend on the user comparing a SAS or a fingerprint. |

## 3. Assets

| ID | Asset | Why it matters |
|---|---|---|
| A1 | Static private key (`identity.key`) | Whoever holds it can impersonate this device to every peer that trusts it. |
| A2 | Trust store integrity (`trusted_devices.json`) | An injected entry makes an attacker's key trusted; a removed entry breaks pairing. |
| A3 | File contents in transit | The data the user is sharing. |
| A4 | Transfer metadata | File names, sizes, count, media types and timestamps can be as sensitive as content. |
| A5 | Sender identity | The sender's static key should not be disclosed to a device it did not intend to reach. |
| A6 | Receiver file system | Existing files must never be overwritten; nothing may be written outside the destination directory; temporary-file namespace. |
| A7 | Local private data | Local paths, OS error text and other local details must not reach the peer. |
| A8 | User attention and consent | Prompts and SAS comparisons are security decisions and must not be bypassed or spoofed. |
| A9 | Terminal and log integrity | Peer-supplied text must not inject escape sequences, fake lines or misleading invisible characters. |
| A10 | Availability | A receiver should keep serving legitimate peers; a sender should not hang. |
| A11 | Device privacy on the LAN | Advertised names and fingerprint hints reveal presence and allow tracking. |
| A12 | Truthful completion status | "Completed" must mean every file was verified and stored. |

## 4. Adversaries

| ID | Adversary | Capabilities | Not assumed |
|---|---|---|---|
| ADV1 | **Passive LAN observer** | Reads all traffic on the local network, including mDNS multicast, preambles and encrypted records, with timing and sizes. | Cannot modify or inject traffic. |
| ADV2 | **Active LAN man-in-the-middle** | Intercepts, drops, modifies, injects, replays and relays traffic (ARP or DHCP spoofing, rogue access point, DNS spoofing of host-name targets). Runs its own Dropumi implementation with its own keys and can terminate both legs of a connection. | Cannot break X25519, ChaCha20-Poly1305, SHA-256 or BLAKE3; does not hold a victim's static private key. |
| ADV3 | **Malicious authenticated peer** | Completes the handshake with its own valid key. It may be trusted (paired earlier, later compromised or hostile) or unverified (accepted by a user in Everyone mode). Sends arbitrary, correctly encrypted messages and runs modified software. May impersonate another device's name. | Does not hold another device's static private key. |
| ADV4 | **Unauthenticated client** | Any host that can open a TCP connection to the listening port. Sends arbitrary bytes, opens many or slow connections, probes the responder. | No key trusted by the target. |
| ADV5 | **Spoofed discovery responder** | Publishes forged mDNS/DNS-SD records: copies another device's instance data, name and `fp` hint, points them at its own addresses, or grinds a key whose fingerprint starts with a chosen 40-bit hint. Usually combined with ADV2 or ADV4. | Cannot forge the Noise handshake for a key it does not hold. |
| ADV6 | **Local unprivileged user** | Another account on the same machine. Reads world-readable files, writes into shared or world-writable directories, binds unused ports, sees the process list. | Not root/administrator; cannot read files with mode 0600 owned by another user. |

## 5. Out of scope

- Malware or an attacker running as the **same OS user**, or with root/administrator rights; a compromised operating
  system, kernel or firmware.
- Physical access, forensic recovery, unencrypted disks, and backups or cloud sync of the configuration directory
  (the key file is plaintext, see R3).
- Network-level denial of service: flooding, jamming, Wi-Fi de-authentication, or networks that block multicast or
  isolate clients.
- Side-channel attacks (timing, power, electromagnetic) against the cryptographic libraries beyond what those libraries
  themselves claim.
- Attackers with a large-scale quantum computer.
- Malicious content inside received files. Dropumi transfers bytes; it does not scan them. (The missing quarantine /
  Mark-of-the-Web tagging is a tracked limitation, R9.)
- Users who confirm a SAS without comparing it, or who add a fingerprint they did not verify.
- A malicious receiver that acknowledges files as STORED and then discards or alters them. The sender cannot verify
  remote storage.
- Supply-chain attacks on dependencies, toolchains or CI. These are handled by the dependency and CI policy
  ([ADR-0010](../adr/0010-dependency-license-and-ci-supply-chain-policy.md)), not by this protocol.
- Traffic analysis of record sizes and timing (acknowledged in R10, not mitigated in v1).

## 6. Security goals, mitigations and tests

Section references (§) point to [`v1.md`](v1.md) unless another document is named.

| ID | Goal | Adversaries | Mitigation | Tests |
|---|---|---|---|---|
| G1 | File contents, file names, device names and static public keys are not visible on the wire. | ADV1 | Noise XX with ChaCha20-Poly1305 encrypts every record after the handshake; both static keys travel encrypted inside the handshake; file metadata is sent only in encrypted Offer/OfferFiles (§6.6, §7, §13.1). | `e2e security::wire_capture_contains_no_plaintext` (content, file-name and both device-name canaries, and both static public keys); `protocol/handshake xx_happy_path_same_hash_and_sas`; `protocol/record roundtrip_both_directions_and_max_size` |
| G2 | An active MitM cannot read or modify a session between devices that trust each other. | ADV2 | Mutual static-key authentication in XX; the sender checks the responder against the pinned fingerprint before msg3; a TrustedOnly receiver denies unknown initiators (§6.7, §12.2, §12.6). | `e2e security::mitm_relay_rejected_by_pinned_trust`; `e2e policy::expect_fingerprint_mismatch`; `e2e policy::trusted_only_rejects_unknown_sender`; `application/channel pin_mismatch_aborts_before_msg3`; `protocol/handshake xx_happy_path_same_hash_and_sas`, `protocol/handshake derived_public_matches_peer_view`, `protocol/handshake initiator_learns_responder_before_msg3` |
| G3 | A MitM during first contact (pairing, Everyone mode, `--allow-unverified`) is detected by the users with high probability. | ADV2 | Committed SAS: the responder commits to `N_R` in msg2, the initiator sends `N_I` in msg3, `N_R` is revealed in Hello_R and checked; 10^-6 success per attempt (§11). | `protocol sas::tests::commitment_kat`, `protocol sas::tests::sas_kat`, `protocol sas::tests::sas_commitment_mismatch_detected`; `protocol/handshake sas_reveal_mismatch_detected`, `protocol/handshake mitm_split_handshakes_sas_independent`; `application/channel roundtrip_same_sas_and_views`; `application/pairing both_confirm_store_trust`, `application/pairing one_denies_stores_nothing`; `e2e pairing::sas_both_confirm_mutual_trust_then_trusted_transfer`, `e2e pairing::sas_deny_stores_nothing`; cancelling instead of answering stores nothing: `application/pairing cancel_while_waiting_for_peer_result`, `e2e pairing::cancel_during_sas_exits_4_and_stores_nothing`. MitM relay between two real processes showing different codes: not covered (planned). |
| G4 | Nobody can deliver files to a receiver without authorization and consent. | ADV3, ADV4 | TrustedOnly by default; Everyone mode is time-boxed, always prompts with the SAS and never creates trust; an unverified peer is never auto-accepted; PAIR only inside an explicit one-shot pairing window (§12.1–§12.4, §12.7). | `domain policy::tests::authorize_matrix_exhaustive`, `domain policy::tests::unverified_never_auto_accepted`; `application/policy unknown_initiator_denied_trusted_only`, `application/policy pair_intent_denied_in_receive`, `application/policy everyone_mode_expires`, `application/policy unverified_offer_always_prompts`, `application/policy receiver_declines`; `application/pairing window_closes_without_attempt`; `application/failures consent_timeout_sends_reject_timeout`; `e2e policy::trusted_only_rejects_unknown_sender`, `e2e policy::receiver_declines`, `e2e policy::accept_never_rejects_without_prompt`, `e2e policy::consent_timeout_rejects`, `e2e transfer::everyone_mode_consent_with_matching_sas`; control decisions bound to the waiting prompt: `cli control::tests::decision_without_a_prompt_is_never_queued`, `cli control::tests::transfer_id_binds_the_decision`, `cli control::tests::abandoned_prompt_takes_no_decision`, `cli interaction::tests::lines_typed_before_a_prompt_are_not_answers`, `e2e policy::stale_accept_never_approves_a_later_offer`. In terminal mode, input still queued in the terminal is flushed before every prompt on Unix (checked by scripted pseudo-terminal runs on macOS 27 arm64 and in a Debian 12 aarch64 container, not by a test in the repository; not implemented on Windows). A pairing window used up by a non-pairing connection: not covered (planned). |
| G5 | A sender never discloses its identity or its files to a receiver it did not intend to reach. | ADV2, ADV5 | The sender aborts before msg3 (its static key is never sent) on an expectation mismatch or an unverified receiver without permission; with `--allow-unverified` it confirms the SAS before sending the Offer (§6.7, §12.5). | `application/channel sender_aborts_before_msg3_for_unverified`, `application/channel pin_mismatch_aborts_before_msg3`; `application/policy unverified_sender_must_confirm_sas`; `protocol/handshake initiator_learns_responder_before_msg3`; `e2e policy::sender_refuses_unverified_receiver`; `e2e security::mitm_relay_rejected_by_pinned_trust` (the impostor never receives msg3) |
| G6 | Device names and discovery data cannot be used to spoof identity. | ADV3, ADV5 | Names and TXT data are hints only; the peer display shows the local trust alias first and labels the Hello name as "claims"; TXT values are validated and sanitized; name targets must be unique (§12.6, §12.8; [`discovery.md`](discovery.md)). | `infra txt::tests::txt_parse_valid`, `infra txt::tests::txt_rejects_bad_fp_hint`, `infra txt::tests::txt_sanitizes_name_64b`; `cli output::tests::human_peer_line_alias_first_claims_label`; `domain discovery::tests::resolve_target_ambiguous`, `domain discovery::tests::parse_targets`; `domain device::tests::strips_and_truncates` |
| G7 | A trusted device cannot impersonate another trusted device by name, mDNS record or address. | ADV3, ADV5 | Name and fingerprint-prefix targets pin the key: the TXT hint or prefix that resolves to a unique trust entry becomes that entry's full fingerprint and is checked before msg3. Address targets are shown with the authenticated alias and display fingerprint. Hello names are labelled "claims" (§12.6, §12.8). | `application/channel name_target_binds_fp_hint`, `application/channel fp_prefix_unique_trust_pins_full_key`; `domain connection::tests::expectation_rules`; `e2e policy::send_by_ip_reports_actual_trusted_peer`; `e2e discovery::mdns_discover_and_send_by_fingerprint` (opt-in; positive path only) |
| G8 | Recorded traffic cannot be replayed, reordered or reflected. | ADV2 | Fresh ephemeral keys per session; implicit per-direction nonces; separate keys per direction; any decryption failure is fatal (§7.2). | `protocol/handshake replayed_msg1_msg3_fail_at_new_responder`; `protocol/record record_replay_fails`, `protocol/record record_reorder_fails`, `protocol/record directions_use_independent_keys`; `e2e security::recorded_session_replay_rejected` |
| G9 | Version negotiation cannot be downgraded. | ADV2 | The exact preamble and reply bytes are bound into the Noise prologue; no automatic retry with a lower version (§6.4, §6.5). | `protocol version::tests::prologue_layout`; `protocol/handshake prologue_mismatch_fails_at_initiator_msg2`; `application/channel preamble_tamper_breaks_handshake`; `e2e security::preamble_tamper_breaks_handshake` |
| G10 | Incompatible versions fail clearly and early. | — | Cleartext preamble with NO_COMMON_VERSION reply (§6.2–§6.4). | `protocol version::tests::preamble_roundtrip`, `protocol version::tests::preamble_rejects_bad_magic_format_count_order`, `protocol version::tests::select_highest_common`; `application/channel busy_and_incompatible_preamble_replies`; `e2e security::incompatible_version_raw_peer` |
| G11 | Tampering with records in transit is detected and ends the session. | ADV2 | AEAD on every record; the opener is poisoned after one failure; no rekey and no nonce reuse (§7.2). | `protocol/handshake tampered_msg2_fails`, `protocol/handshake tampered_msg3_fails`; `protocol/record record_bitflip_fails`, `protocol/record opener_poisoned_after_failure`, `protocol/record truncated_and_short_records_fail`; `protocol record::tests::nonce_exhaustion_errors`; `e2e security::ciphertext_bitflip_aborts` |
| G12 | A corrupted or truncated file is never reported as stored, and "completed" means every file was verified and stored. | ADV2, ADV3 | BLAKE3-256 per file; exact byte accounting; STORED only after hash check, fsync and no-clobber finalization; explicit Complete/CompleteAck after a directory fsync; end-of-stream without an end message is `connection_lost` (§13.4–§13.6, §15). | `protocol session::receiver::tests::hash_mismatch_reports_integrity_failed`, `protocol session::receiver::tests::receiver_rejects_short_file_end`, `protocol session::receiver::tests::receiver_rejects_data_beyond_size`, `protocol session::receiver::tests::zero_byte_file_ok`, `protocol session::receiver::tests::cancel_after_complete_ack_ignored`; `protocol/session_props valid_sender_trace_accepted`; `domain transfer::session::tests::completion_requires_all_stored_and_settled`; `application/transfer roundtrip_trusted_auto_accept`, `application/transfer progress_reaches_total_only_on_completion`; `application error::tests::io_kinds_map_to_connection_lost`; `e2e security::wrong_file_hash_integrity_failed`, `e2e transfer::single_file_trusted_auto_accept`, `e2e transfer::large_file_streaming`, `e2e transfer::many_small_files`, `e2e failure::receiver_killed_sender_reports_connection_lost` |
| G13 | Hostile file names cannot escape the destination directory or spoof their appearance. | ADV3 | Wire validation (single component, no `/` `\` NUL, not `.`/`..`); sanitization (last component, invisible and control characters removed, NFC, reserved Windows names and characters, trailing dots and spaces, `.dropumi-` prefix, ≤255 B) (§18.1, §18.2). | `domain filename::tests::sanitize_fixtures`, `domain filename::tests::wire_name_validation`; `domain/filename_props sanitized_is_safe`, `domain/filename_props sanitized_is_safe_any_string`; `domain text::tests::strips_cc_zl_zp_cf_keeps_zwj_zwnj_vs`, `domain text::tests::cf_table_sorted_disjoint`, `domain text::tests::unicode_version_matches_normalization`; `domain filename::tests::truncation_cannot_resurrect_device_names`, `domain filename::tests::hfs_plus_decomposed_length_bounded`, `domain filename::tests::tag_survives_long_names`; `protocol message::tests::rejects_bad_values`; `e2e security::traversal_and_reserved_names`, `e2e security::malformed_traffic_matrix` |
| G14 | Existing files are never overwritten and symlinks are never followed. | ADV3, ADV6 | Exclusive temp-file creation in the destination directory; no-clobber finalization with `name (n).ext` suffixes; Windows retry for transient errors; Unix placeholder fallback that removes only its own empty placeholder (§18.3, §18.4). | `infra fs::receive_dir::tests::persist_never_overwrites_existing`, `infra fs::receive_dir::tests::case_variant_collision_keeps_original`, `infra fs::receive_dir::tests::symlink_at_final_name_not_followed`, `infra fs::receive_dir::tests::dangling_symlink_not_replaced`; `infra fs::finalize::tests::collision_suffix_sequence`, `infra fs::finalize::tests::placeholder_fallback_never_overwrites`, `infra fs::finalize::tests::placeholder_removed_when_second_persist_fails`, `infra fs::finalize::tests::other_errors_fail_immediately`; `domain filename::tests::collision_suffixes`; `domain/filename_props collision_suffix_bounded`; `application/transfer duplicate_names_get_collision_suffixes`; `e2e transfer::existing_file_never_overwritten`, `e2e transfer::multiple_files_empty_unicode_duplicates`. Windows retry path: not covered (planned; Windows-only tests, pending CI). |
| G15 | Oversized or malformed input cannot cause excessive memory use, crashes or undefined behaviour. | ADV3, ADV4 | Lengths checked before allocation; strict, bounded CBOR (definite lengths, ascending keys, ≤32 entries, depth ≤4, no tags or floats, no trailing bytes); per-field limits; 10,000 files and 4 MiB of offer metadata; ≤16 consecutive ignorable messages; fixed per-session buffers; no `unsafe` code (§7.1, §8, §17.1). | `protocol record::tests::len_prefix_bounds`; `protocol frame::tests::frame_roundtrip_and_bounds`; `protocol/handshake handshake_msg_over_cap_rejected`, `protocol/handshake msg1_with_payload_rejected`; `protocol codec::tests::rejects_indefinite`, `protocol codec::tests::rejects_duplicate_or_unsorted_keys`, `protocol codec::tests::rejects_tags_floats_undefined`, `protocol codec::tests::rejects_depth_over_4`, `protocol codec::tests::rejects_trailing_bytes`, `protocol codec::tests::enforces_string_and_array_limits`; `protocol message::tests::ignores_unknown_keys`, `protocol message::tests::ignorable_type_skipped`, `protocol message::tests::unknown_required_type_unsupported`, `protocol message::tests::rejects_bad_values`; `protocol/codec_props decode_arbitrary_never_panics`, `protocol/codec_props mutated_valid_never_panics`, `protocol/codec_props message_roundtrip`; `protocol session::receiver::tests::receiver_rejects_too_many_files_and_metadata_over_limit`, `protocol session::receiver::tests::receiver_rejects_empty_data_wrong_direction_bad_names_and_flood`, `protocol session::receiver::tests::receiver_rejects_count_mismatch`, `protocol session::receiver::tests::receiver_rejects_total_mismatch_or_overflow`, `protocol session::receiver::tests::offer_batches_fit_one_record`; `protocol/session_props random_inputs_never_panic`; `application/channel oversized_length_rejected_preauth`; `domain/identity_props parsers_never_panic`, `domain/identity_props target_classification_is_consistent`, `infra/txt_props txt_parse_is_bounded`; `e2e security::malformed_traffic_matrix`. Memory independent of file size: `e2e transfer::bounded_memory_independent_of_size` (Linux only; passed in the Debian 12 aarch64 container, pending CI). |
| G16 | Out-of-order or wrong-state messages are rejected. | ADV3 | Sans-IO session state machines with an explicit allowed-message set per state; direction checked for every message (§13.5, §14). | `protocol session::receiver::tests::receiver_rejects_data_before_accept`, `protocol session::receiver::tests::receiver_rejects_out_of_order_index`, `protocol session::receiver::tests::receiver_rejects_complete_with_pending`, `protocol session::receiver::tests::receiver_rejects_empty_data_wrong_direction_bad_names_and_flood`, `protocol session::receiver::tests::abort_and_write_failed_paths`; `protocol message::tests::wrong_direction_rejected`; `protocol/session_props valid_sender_trace_accepted`; `domain transfer::session::tests::transition_table_exhaustive`, `domain transfer::request::tests::request_invariants`, `domain transfer::progress::tests::progress_invariants`, `domain connection::tests::state_transitions`; `e2e security::malformed_traffic_matrix`, `e2e security::ignorable_message_during_consent_is_skipped` (ignorable types are skipped, not fatal, while the user decides), `e2e security::peer_error_before_offer_reports_its_code`. Sender-side rejection of unexpected FileAck and CompleteAck messages: not covered (planned). |
| G17 | Connection floods and slow clients cannot exhaust the receiver. | ADV4 | Admission limits (16 pre-auth global, 4 per IP, 4 sessions, 1 per fingerprint); 10 s deadline from admission to Hello_R; one absolute deadline for the whole offer (30 s trusted, 150 s unverified); 30 s idle timeout while streaming (§13.2, §17.2, §17.3). | `application usecases::admission::tests::admission_limits_global_per_ip_and_session`; `application/channel busy_and_incompatible_preamble_replies`, `application/channel oversized_length_rejected_preauth`; `application/failures idle_timeout_fails_session`, `application/failures offer_deadline_is_absolute`; `infra fs::finalize::tests::probe_cap_switches_to_random_name`, `infra fs::receive_dir::tests::duplicate_names_probe_from_hint` (bounded finalization work for duplicate names); `e2e security::malformed_traffic_matrix` (the receiver keeps serving afterwards). The 10 s pre-auth deadline and slot recovery against silent connections: not covered (planned). |
| G18 | A stalled receiver cannot hang the sender. | ADV3, ADV4 | Write-stall timeout of 120 s on every pending record write; 150 s decision deadline; 60 s CompleteAck deadline, restarted by each new FileAck (§13.6, §17.3). | `application/failures stalled_receiver_times_out_sender`, `application/failures complete_ack_deadline_restarts_on_each_file_ack` (a slow but progressing receiver is not timed out), `application/failures complete_ack_deadline_still_fires`. Decision deadline: not covered (planned). |
| G19 | A local unprivileged user cannot read the identity key or inject trust. | ADV6 | Unix: the configuration directory, `identity.key`, `device.json` and `trusted_devices.json` must be owned by the effective user and must not be symbolic links; the directory and the two JSON files must not be group- or world-writable; the key file must have no group/other bits at all; any violation exits with code 9. Windows: warning when the configuration directory is outside `%LOCALAPPDATA%`. Received files are 0600 on Unix. | `infra store::identity_file::tests::identity_created_0600_in_0700`, `infra store::identity_file::tests::identity_rejects_group_readable`, `infra store::identity_file::tests::identity_bad_magic_rejected`, `infra store::identity_file::tests::concurrent_create_single_identity`; `infra perms::tests::config_dir_rejects_world_writable`, `infra perms::tests::config_dir_outside_localappdata_detected`; `infra store::trust_file::tests::trust_store_rejects_group_writable`, `infra store::trust_file::tests::concurrent_writers_serialized`, `infra store::trust_file::tests::corrupt_file_reported`; `infra store::atomic::tests::atomic_replace_no_temp_left`; `infra fs::receive_dir::tests::received_file_mode_0600`. Exit code 9 from the CLI end to end: not covered (planned). |
| G20 | No local data leaks to the peer through error messages. | ADV3 | `Error.detail` is a fixed static phrase per code; storage errors are signalled only as `FileAck WRITE_FAILED` and `Cancel STORAGE` (§16.2). | `protocol message::tests::wire_details_are_static_and_pathless`; `application/failures storage_error_detail_not_leaked`; `application/transfer write_failure_cancels_with_storage`. Local `tracing` logs also carry no paths: `application error::tests::store_error_kind_has_no_path`, `application/failures trust_store_errors_are_logged_without_paths`, `application/failures last_seen_failures_are_logged_without_paths` |
| G21 | Peer-supplied text cannot spoof the terminal or logs. | ADV3, ADV5 | Shared stripping of Cc, Zl, Zp and Cf (except ZWJ/ZWNJ) plus escape-sequence removal on every peer string shown or logged; JSON escaping in NDJSON output; received Error details are never shown (§12.8, §18.2). | `cli output::tests::terminal_safe_strips_ansi_bidi_and_invisibles`, `cli output::tests::ndjson_escapes_peer_strings`, `cli output::tests::human_peer_line_alias_first_claims_label`; `domain device::tests::strips_and_truncates`; `domain text::tests::strips_cc_zl_zp_cf_keeps_zwj_zwnj_vs`, `domain text::tests::truncate_respects_char_boundaries`; `domain/name_props device_name_is_safe_and_idempotent`, `domain/name_props device_name_from_any_string`, `domain/name_props strip_unsafe_removes_everything_unsafe`; `cli output::tests::terminal_safe_output_is_inert` |
| G22 | Partial data does not linger after cancellation, failure or a crash, and cancellation is reported truthfully. | ADV3 | Temporary files are discarded on every failure path; acknowledged files are kept; `Cancel USER` for a user's cancel, `Cancel SHUTDOWN` when the receiver stops; stale `.dropumi-*.part` files older than 15 minutes are removed at startup (§14.2, §16.1, §18.3). | `application/failures discard_called_on_every_failure_path`, `application/failures sender_cancel_mid_file_discards_partial_keeps_acked`, `application/failures receiver_cancel_mid_file` (SHUTDOWN), `application/failures idle_timeout_fails_session`; `application/transfer source_changed_sends_file_abort`; `infra fs::receive_dir::tests::temp_removed_on_drop`, `infra fs::receive_dir::tests::discard_removes_temp`; `infra fs::cleanup::tests::stale_cleanup_pattern_and_age_only`; `e2e failure::sender_cancels_mid_transfer`, `e2e failure::receiver_cancels_mid_transfer`, `e2e failure::sender_killed_receiver_cleans_up`, `e2e failure::stale_parts_cleaned_on_start` |
| G23 | Stale or spoofed candidate addresses do not prevent reaching the intended peer. | ADV5 | Manual addresses are tried first, in the user's order; candidates are tried one at a time with a 3 s connect timeout each; identity is decided by the handshake, never by the address ([`discovery.md`](discovery.md) §8). | `domain connection::tests::rank_candidates_order`; `application/failures connect_tries_candidates_in_rank_order`; `infra net::tests::connect_first_skips_dead_candidate`; `e2e failure::retry_skips_dead_candidate`, `e2e failure::connect_refused_exit_5` |
| G24 | The identity and its fingerprints are computed correctly and consistently. | — | X25519 per RFC 7748; fingerprint and text forms per §5; the secret never appears in debug output. | `protocol identity::tests::x25519_rfc7748_vector`, `protocol identity::tests::fingerprint_kat`, `protocol identity::tests::debug_redacts_secret`; `protocol/handshake derived_public_matches_peer_view`; `domain identity::tests::display_and_parse_roundtrip`, `domain identity::tests::parse_rejects_bad_input`, `domain identity::tests::prefix_matching`; `infra store::identity_file::tests::reset_replaces_key` |

## 7. Residual risks

These risks remain with the v1 design. They are disclosed in the README and the pull request.

| ID | Risk | Notes |
|---|---|---|
| R1 | **Responder static key can be probed.** Anyone who can reach a listening device learns its static key and fingerprint from msg2 (XX responder identity hiding is weak against active probing). | Accepted for v1. Combined with R2 this allows tracking a receiver across networks. |
| R2 | **mDNS TXT data allows tracking.** `n` and `fp` are stable and broadcast in cleartext to the whole link. | Users can run with `--no-mdns`. A rotating, trust-keyed hint is a future idea ([`discovery.md`](discovery.md) §11). |
| R3 | **The identity key is a plaintext file** (mode 0600 on Unix, profile ACL on Windows). It is exposed to malware running as the same user, to backups and to unencrypted disks. Dropumi zeroizes its own in-memory copy; `snow` 0.10.0 keeps its internal copies in plain arrays and does not zeroize them (checked in the crate source; [ADR-0003](../adr/0003-device-identity-and-key-storage.md)). | OS keychain storage is planned for Phase 2. |
| R4 | **Unaudited protocol and implementation.** v1 is a draft with no third-party review; `snow` states it has not been formally audited. | Do not use for sensitive data yet. |
| R5 | **A peer reached by IP address or host name is reported, not pinned.** If a user sends to an address, the connection goes to whichever trusted device answers there. | The sender reports the authenticated alias and display fingerprint (G7); `--expect-fingerprint` adds a pin. |
| R6 | **Placeholder race on exFAT/vfat.** On Unix file systems without an exclusive rename, a short window exists between creating the placeholder and renaming onto it, which a local user with write access to the destination directory could race. | Tested only with injected errors; behaviour on real exFAT/vfat is UNVERIFIED. |
| R7 | **The SAS depends on people.** The committed SAS bounds a MitM to 10^-6 per attempt only if users compare the codes; Everyone mode and `--allow-unverified` are exposed if they do not. | Pairing and SAS confirmation can never be confirmed by a flag or policy. |
| R8 | **No prompt rate limiting in Everyone mode.** Any LAN host can trigger consent prompts while Everyone mode is on (bounded only by the session limits). | Everyone mode is time-boxed (10 minutes by default). |
| R9 | **Received executables are not quarantined.** Files are not tagged with the macOS quarantine attribute or the Windows Mark-of-the-Web, so opening a received program may not trigger the Gatekeeper or SmartScreen checks that a download would. | Phase 2 security priority. |
| R10 | **Traffic analysis.** Records are not padded, so record sizes and timing reveal approximate file sizes, file counts and transfer duration; the cleartext preamble reveals which protocol versions each side implements. | Accepted for v1. |
| R11 | **No key rotation or revocation.** A stolen key stays usable against every peer that trusts it until each peer removes the entry by hand. `identity reset` creates a new identity. | Future capability. |
| R12 | **Windows configuration ACLs are not inspected.** With an explicit `--config-dir`, the directory inherits its parent's ACL; Dropumi only warns when it is outside `%LOCALAPPDATA%`. | Documented in [`SECURITY.md`](../../SECURITY.md) and [ADR-0003](../adr/0003-device-identity-and-key-storage.md). |
| R13 | **Unauthenticated hosts can occupy capacity.** A LAN host can hold pre-auth slots (16 global, 4 per IP) for up to 10 s each. Any TCP connection to `pair --listen` uses up its one-shot pairing window, forcing the user to reopen it. | Neither leads to trust: pairing still requires a matching SAS confirmed on both devices. |
| R14 | **The 40-bit mDNS hint is copyable and grindable.** An attacker can advertise someone else's hint, or grind a key that matches it. | The hint never authenticates. If it resolves to a unique trust entry, the full fingerprint is pinned (G7); otherwise the peer is unverified and policy applies. |
| R15 | **Trusted peers can fill the disk.** v1 has no disk-space pre-check, and a receiver with `--accept trusted` accepts any offer from a trusted peer (up to 10,000 files, at most 2^63 − 1 bytes in total). | Running out of space aborts the transfer (`WRITE_FAILED`, `Cancel STORAGE`). Reject reason 4 is reserved for a future pre-check. |
| R16 | **Default port squatting.** A local user (or another program) can bind TCP 47321 first. The receiver then falls back to an ephemeral port, and senders that address `host:47321` by hand reach the squatter. | The squatter does not hold the receiver's key, so a pinned sender aborts before msg3 and an unverified one is refused by policy. The impact is denial of service and confusion. |
| R17 | **Directory-entry durability on Windows** relies on the file system rather than an explicit directory fsync. | UNVERIFIED; documented in [`v1.md`](v1.md) §15.3. |
| R18 | **Limited test environment.** Automated tests cover loopback only and have run only on the maintainer's macOS 27 arm64 machine and in a Debian 12 aarch64 container on it; Windows code was cross-compiled but not run. The GitHub-hosted runners (Ubuntu 24.04 x64, macOS 26 arm64, Windows Server 2025 x64) are "Not tested (pending CI)". Real multi-device LAN behaviour is "Not tested". mDNS on CI runners, Windows Firewall prompts, Avahi coexistence and macOS Local Network privacy are UNVERIFIED. | See [`docs/testing.md`](../testing.md). |
| R19 | **Some malformed input ends a session without an Error message.** During the Hello exchange, and when an offer, idle or write-stall timer expires, the reference implementation closes the connection without telling the peer why. | The peer reports `connection_lost`, `handshake_failed` or `timeout`; no data is lost or exposed, but diagnosis is harder. |

## 8. Revision history

| Date | Change |
|---|---|
| 2026-10-13 | Initial draft for protocol v1 (milestone 1). |
| 2026-10-13 | Test references replaced with the exact names from the milestone 1 test run, gaps marked "not covered (planned)", test-status paragraph added, pairing-window and cancellation behaviour aligned with the code, R19 added; G19 lists the files that are actually checked; R3 states `snow`'s key handling from its crate source. |
| 2026-10-14 | Tests added after the code review and the completeness audit referenced (G3, G4, G13, G15 to G18, G20, G21); test-status paragraph updated to the final run, including the Linux container run. |
