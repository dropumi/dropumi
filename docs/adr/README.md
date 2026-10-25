# Architecture Decision Records

This directory records the significant architecture, security, protocol and governance decisions for Dropumi. Each record explains the context, the decision, its consequences and the alternatives that were rejected, so that later contributors can see why the code looks the way it does.

> Dropumi is pre-alpha. The protocol and implementation are unaudited. These records cover milestone 1 (Phase 0 + Phase 1: secure LAN transfer core, protocol v1 draft and the `dropumi` CLI) and milestone 2 (the FFI for the native apps, the apps, and the browser companion design). On 2026-10-13 every record was reconciled with the implemented code; where a draft and the code disagreed, the record now describes the code. No platform is labelled "supported"; see [`../feasibility.md`](../feasibility.md) and [`../testing.md`](../testing.md) for what has actually been tested.

## Index

| ADR | Title | Status | Implemented in |
|---|---|---|---|
| [0001](0001-architecture-and-workspace-layout.md) | Architecture and workspace layout | Accepted | `Cargo.toml`, `crates/*`, `scripts/ci/check-architecture.sh` |
| [0002](0002-secure-channel-noise-xx.md) | Secure channel: Noise XX over TCP | Accepted | `crates/protocol/src/{handshake,record}.rs`, `crates/application/src/channel/` |
| [0003](0003-device-identity-and-key-storage.md) | Device identity and key storage | Accepted | `crates/protocol/src/identity.rs`, `crates/infra/src/{store,perms.rs,paths.rs}` |
| [0004](0004-pairing-trust-and-transfer-policy.md) | Pairing, trust and transfer policy | Accepted | `crates/protocol/src/sas.rs`, `crates/domain/src/{policy,connection}.rs`, `crates/application/src/usecases/pair.rs` |
| [0005](0005-wire-protocol-v1.md) | Wire protocol v1 | Accepted (draft protocol) | `crates/protocol/src/`, `crates/application/src/config.rs` |
| [0006](0006-integrity-and-receiver-filesystem-safety.md) | Integrity and receiver filesystem safety | Accepted | `crates/domain/src/{filename,text}.rs`, `crates/infra/src/fs/` |
| [0007](0007-lan-discovery-mdns.md) | LAN discovery via mDNS | Accepted | `crates/infra/src/{mdns,txt}.rs` (feature `mdns`) |
| [0008](0008-ui-framework-strategy.md) | UI framework strategy | Superseded by ADR-0012 | Mobile: native (ADR-0012); desktop beyond the Mac: not implemented |
| [0009](0009-ffi-bindings-strategy.md) | FFI and bindings strategy | Accepted | `crates/ffi/`, `crates/ffi-bindgen/`, `scripts/ffi/`, `apps/apple/DropumiKit/`, `apps/android/core-bindings/` |
| [0010](0010-dependency-license-and-ci-supply-chain-policy.md) | Dependency, license and CI supply-chain policy | Accepted | `deny.toml`, `.github/workflows/`, `.github/dependabot.yml` |
| [0011](0011-browser-companion.md) | Browser companion | Proposed | `poc/web/` (research prototype, part 1) |
| [0012](0012-apple-apps.md) | Apple apps (iPhone, iPad, Mac) in SwiftUI | Accepted | `apps/apple/`, `scripts/apple/`, `crates/cli/tests/e2e/interop/apple.rs` |
| [0013](0013-android-app.md) | Android app (phone and tablet) in Kotlin and Jetpack Compose | Proposed | `apps/android/`, `scripts/android/`, `crates/cli/tests/e2e/interop/android.rs` (milestone 2c-1, the foreground app; receiver mode, 2c-2, not implemented) |
| [0014](0014-interim-key-storage-on-app-platforms.md) | Interim key storage on the app platforms | Accepted | `crates/ffi/src/node.rs` (configuration directory from the app); platform settings in the apps |

ADR-0001 to ADR-0010 are dated 2026-10-13; ADR-0009 was accepted, and ADR-0011 and ADR-0014 were written, on 2026-10-14 (milestone 2). ADR-0012 (Apple apps) was written on 2026-10-15 and supersedes ADR-0008. ADR-0013 (Android app) was proposed on 2026-10-15 for milestone 2c and updated on 2026-10-16 with the 2c-1 implementation and the spike results; it stays Proposed until the maintainers review it.

Related normative and supporting documents:

- [`../protocol/v1.md`](../protocol/v1.md): the normative wire protocol (ADR-0005 summarises the decisions behind it).
- [`../protocol/threat-model.md`](../protocol/threat-model.md): assets, adversaries, mitigations, tests and residual risks.
- [`../protocol/discovery.md`](../protocol/discovery.md): mDNS service, TXT keys and privacy cost.
- [`../../ARCHITECTURE.md`](../../ARCHITECTURE.md): crate map, ports and runtime model.
- [`../dependencies.md`](../dependencies.md): every direct dependency with its license and justification.

## Status values

| Status | Meaning |
|---|---|
| Proposed | Under discussion, or agreed in direction but not yet in effect (for example, it applies to a later phase). |
| Accepted | In effect. Code and docs must follow it. |
| Accepted (provisional) | In effect as the default, but explicitly gated on evidence that has not been gathered yet. The record names the gates. |
| Accepted (draft protocol) | In effect for the current draft of a wire protocol. The protocol itself may still change before it is declared stable. |
| Superseded by ADR-NNNN | Replaced. The record stays for history; the new record links back. |
| Deprecated | No longer applies and has no direct replacement. |
| Rejected | Considered and declined. Kept so the discussion is not repeated. |

## Evidence labels

Records use the project's labels exactly, with an ASCII hyphen:

- test status: `Automated test passed`, `Manual device test passed`, `Build verified only`, `Not tested`, `Unsupported`;
- capability research: `Documented capability - not tested`, `Requires investigation`, `Unsupported`, `Not applicable`.

A label may carry a qualifier in parentheses, for example "Automated test passed (CLI, loopback, local run)" or "Not tested (pending CI)". Milestone 1 evidence comes from local runs on the maintainer's machine (macOS 27 arm64, Rust 1.99.0); CI has not run yet, so every GitHub-runner result is "Not tested (pending CI)".

## Process

1. Copy the template below into `NNNN-short-kebab-title.md`, using the next free number. Numbers are never reused.
2. Start with `Status: Proposed` and open a pull request. Decisions that touch the protocol, cryptography, trust model or filesystem safety need a written threat analysis in the record and a maintainer review.
3. Once merged as Accepted, a record is not rewritten. To change a decision, write a new record and mark the old one `Superseded by ADR-NNNN`. Fixing typos, broken links or reconciling a record with the code that implements it is allowed and noted in the PR.
4. Accepted records end with an **Implementation** section that names the code paths and tests behind the decision.
5. Keep claims honest: no invented benchmark numbers, no "supported" platform labels, and mark anything that was not checked as UNVERIFIED.

## Template

```markdown
# ADR-NNNN: Title

- **Status:** Proposed
- **Date:** YYYY-MM-DD
- **Related:** ADR-XXXX, links to specs or issues

## Context

What problem or force makes a decision necessary? Which requirements, constraints
and facts (with sources) apply? What is unknown or UNVERIFIED?

## Decision

What we will do, stated concretely enough to implement and to check in review:
versions, parameters, limits, defaults, and how the decision is enforced
(lint, CI job, test).

## Implementation

(Once accepted.) Code paths, tests and evidence labels.

## Consequences

### Positive

- ...

### Negative

- ... (include residual risks and follow-up work)

## Alternatives considered

- **Alternative A:** one line on why it was not chosen.
- **Alternative B:** one line on why it was not chosen.
```
