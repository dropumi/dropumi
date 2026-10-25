<!--
The PR title must be a Conventional Commit, for example "fix(infra): retry finalization on sharing violations".
Never paste personal file names, full paths, user names, host names, keys, fingerprints of real devices, or IP/MAC addresses.
Security vulnerabilities are reported privately (SECURITY.md), never in a pull request.
-->

## Summary

<!-- What does this change do, and why? -->

## Type of change

- [ ] `feat`: new functionality
- [ ] `fix`: bug fix
- [ ] `refactor`: no behaviour change
- [ ] `docs`
- [ ] `test`
- [ ] `perf`
- [ ] `chore` / `ci`
- [ ] Breaking change (wire protocol, CLI, exit codes, or on-disk formats)

## Linked issue

<!-- "Closes #123", or "None". -->

## Architecture impact

<!--
Which crates or layers change (domain, protocol, application, infra, cli)?
New ports, adapters or dependencies? Does the dependency direction still point inward?
New or updated ADR (docs/adr/)?
-->

## Security and privacy

<!--
Does this touch the handshake, record layer, pairing/SAS, trust policy, peer input limits, receiver filesystem safety,
config permissions, logging, or what is broadcast over mDNS? How is it protected and tested?
Write "None" if not applicable.
-->

## Tests run

<!--
Paste the exact commands you ran and their real results (summary lines). Replace your home directory with ~.
Delete commands you did not run; never list a command without its real result. The full list is in CONTRIBUTING.md.
-->

```text
$ cargo fmt --all --check
$ git ls-files -z -- '*.rs' | xargs -0 awk 'FNR == 1 && $0 != "// SPDX-License-Identifier: MPL-2.0" { print FILENAME; bad = 1 } END { exit bad }'
$ cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
$ cargo clippy --workspace --all-targets --no-default-features --locked -- -D warnings
$ RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked
$ cargo test --workspace --all-features --locked
$ cargo deny --locked check
$ bash scripts/ci/check-architecture.sh
```

## Platform verification

<!--
Use only these labels: Automated test passed, Manual device test passed, Build verified only, Not tested, Unsupported.
Name the OS version, the architecture, and the environment (CI runner, local machine, physical device). Never write "supported".
A result on one OS never stands for another. Until CI has run on this PR, CI rows read "Not tested (pending CI)".
Apps: name the image or device for each run, for example "Automated test passed (Android 16, API 36.1, arm64-v8a, Play emulator, macOS 27 arm64 host)". An emulator result is not a device result, an x86_64 result never stands for arm64-v8a, and one API level never stands for another.
-->

| Platform | OS version / architecture / environment | Status |
|---|---|---|
| Linux | | Not tested |
| macOS | | Not tested |
| Windows | | Not tested |
| Android emulator (one row per API level, ABI and image) | | Not tested |
| Android device (model, Android version) | | Not tested |
| iOS / iPadOS | | Not tested |
| Two devices on a real LAN | | Not tested |

## Performance

<!-- Measured numbers with the method used, or "not measured". Never estimate and present it as a measurement. -->

## Screenshots

<!-- UI changes only. Remove device names, file names and fingerprints. Otherwise delete this section. -->

## Known limitations and follow-ups

<!-- What is incomplete, unverified or deferred? -->

## AI assistance

<!-- If AI tools produced a significant part of this change (code, tests or docs), say which parts. Otherwise write "None". -->

## Checklist

- [ ] The branch uses an allowed prefix (`feat/`, `fix/`, `refactor/`, `docs/`, `test/`, `perf/`, `chore/`, `ci/`) and the commits follow Conventional Commits.
- [ ] `cargo fmt --all --check` passes.
- [ ] `cargo clippy` passes with `--all-features` **and** with `--no-default-features` (`-D warnings`).
- [ ] `cargo doc` passes with `RUSTDOCFLAGS="-D warnings"`.
- [ ] `cargo test --workspace --all-features --locked` passes.
- [ ] `bash scripts/ci/check-architecture.sh` passes (dependencies still point inward).
- [ ] Every new `.rs` file has `// SPDX-License-Identifier: MPL-2.0` as its first line.
- [ ] New dependencies are justified, listed in `docs/dependencies.md`, and pass `cargo deny --locked check` (or there are none).
- [ ] Documentation is updated (`docs/`, ADRs, `docs/protocol/` for wire or security changes) and `CHANGELOG.md` `[Unreleased]` is updated for user-visible changes.
- [ ] Android changes: the JVM tests (`:core-bindings`, `:core`, `:appstate`, `:testsupport`, `:app`), `:app:lintRelease`, `scripts/android/check-manifest.sh` and `scripts/android/check-logging.sh` pass; `scripts/android/ui-tests.sh` (and `scripts/android/interop.sh` for core or harness changes) ran on each image listed above, or that row says Not tested.
- [ ] Android changes: no URI, path, peer string or exception text reaches logcat, and no new permission, exported component or provider was added without updating the manifest audit and ADR-0013.
- [ ] No secrets, keys, trust stores, personal files or personal data are included.
- [ ] Significant AI assistance is disclosed above.
- [ ] I have the right to submit this contribution, and it is licensed under MPL-2.0.
