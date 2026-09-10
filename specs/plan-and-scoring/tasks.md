# Tasks

- [x] Plan domain, persistence, selection and CLI.
- [x] Scoring domain, validation, exact calculator and Senior fixture.
- [x] CLI/MCP/snapshot integration and standalone AI guard.
- [x] Shared skill, architecture documentation and workspace setup instructions.
- [x] Scoring/plan/protocol regression tests and requested quality gates.

## Verification, 2026-09-10

- `cargo fmt --check`: PASS.
- `cargo clippy --all-targets --all-features -- -D warnings`: PASS.
- `cargo test`: PASS, 68 tests.
- `cargo test --all-features`: PASS, 71 tests.
- `cargo build --release`: PASS.
- Installer `npm run typecheck`, `npm run lint`, `npm run build`: PASS.
- Installer `npm test`: PASS, 17 tests.
- Release binary CLI smoke in temporary workspace: plan set/show/list/clear,
  scoring show, calculate from stdin and file: PASS.
- Release MCP subprocess: initialize, scoring_get, kpi_calculate: PASS.
- Verified 170000 at 21/21; 161905 at 20/21, breakdown
  38095 / 47619 / 19048 / 19048 / 38095; factor presentation 0.952381.
- Pure calculation preserved workspace bytes and created no credential directory.
- Snapshot integration tests use mocked Bitrix; references and scoring are present,
  shared plan works for selected team members, document contents are absent.
- Shared source skill and all three generated bundles match exactly.
- No Cargo dependency/lockfile changes. Source differences reviewed against a
  pre-edit local copy because this checkout has no Git metadata.

Manual boundary: real Codex/Claude host activation, Drive connector access, actual
team section/revision matching, Bitrix live evidence and trusted time inputs remain
unverified. The user's personal workspace/configuration was not changed. Senior
configuration remains an opt-in example; temporary smoke workspace was removed.
