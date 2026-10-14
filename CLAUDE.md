@AGENTS.md

## Additional rules for this tool

[AGENTS.md](AGENTS.md) is the single source of rules. If this file and AGENTS.md ever disagree, AGENTS.md wins.

- **Use plan mode first** for any change to the protocol or security surface, and get the plan reviewed before you edit. That surface is:
  - `crates/protocol/`, including the handshake, record layer, SAS and codec;
  - pairing and trust policy;
  - secure-channel establishment;
  - receiver filesystem safety;
  - config permissions;
  - the FFI's prompt and answer path (`crates/ffi/src/prompts.rs`), its file-source guards and its discovery rules, and the apps' consent and code screens;
  - `docs/protocol/`.
- **Paste real command output** (with `$HOME` replaced by `~`).
  - Never claim a test, build or benchmark result you did not run.
  - Never summarize output you did not see.
- **Never run `gh pr merge`.** Never push to `main`, never force-push, and never change repository settings or rulesets.
