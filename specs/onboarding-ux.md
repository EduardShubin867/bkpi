# BKPI 0.6 onboarding refinement

Goal: let users configure Bitrix and visible KPI documents without internal IDs.
Users can choose themselves or employees, import or reference an existing text
document, defer KPI entry, and later inspect/change the reference.

Requirements and contract:
- Generate integration IDs from hostname with conservative Bitrix suffix removal
  and numbered collisions; retain explicit CLI/config IDs and masked secrets.
- Keep Workspace/Person/Integration/MCP schemas. New KPI references are relative
  to the project (KPI.md or kpi/<slug>.md); legacy people/... references remain
  relative to .bkpi. Absolute/external references are supported. State paths keep
  their existing confinement. Runtime documents remain UTF-8 text.
- Init collects all people and KPI choices before creating workspace files.
  Copy defaults to yes, never overwrites or deletes a source. Missing documents
  produce the requested Russian placeholder. Cancellation creates no workspace.
- TTY uses selections and confirmations; piped stdin remains line-oriented.
  JSON commands retain clean stdout. Summary includes full resolved KPI paths.
- KPI set accepts [person] path; show accepts [person] and reports existence and
  semantic emptiness, with optional JSON. Multiple people require explicit choice.
- Chat KPI takes precedence, empty local KPI is not a broken workspace, and
  persistent document writes require explicit user consent.

Non-goals: architecture changes, Bitrix writes, binary document parsing, migration
of existing KPI files, changes to installed user credentials or agent settings.

Plan: path/document helpers; wizard and auto IDs; CLI set/show; shared skill and
installer routing/docs; regression tests and isolated setup/init verification.

Acceptance: cover hostname/collision, visible single/team files, legacy loading,
copy/reference/placeholder, whitespace/tilde/quoted paths, source preservation,
set/show, secret-free output, source skill and generated bundles.

Validation: cargo fmt --check; cargo clippy --all-targets --all-features -- -D
warnings; cargo test; cargo build --release; installer typecheck/lint/test/build.
Exercise terminal wizard with a local Bitrix fixture; do not contact live Bitrix.

## Completed validation (2026-09-09)

- cargo fmt --check: passed.
- cargo clippy --all-targets --all-features -- -D warnings: passed.
- cargo test: 37 passed; cargo test --all-features: 39 passed.
- cargo build --release: passed.
- Installer npm run typecheck, npm run lint, npm test (9 passed), npm run build: passed.
- Real macOS PTY: standalone init with skipped KPI; local installed runtime through
  installer setup (CLI-only) then init with copied UTF-8 file containing spaces.
  Exact resolved destination printed; imported bytes equal retained source.
  Transcript: onboarding-smoke.txt. Test integration used a local HTTP fixture
  seeded into an isolated config, never a live portal or the user's config.
- Source comparison against pre-change copies: Bitrix, credentials, MCP and
  snapshot implementation unchanged. No Git metadata is available in this tree.
- Bundled Codex/Claude skills regenerated and checked against common source.
- Runtime remains a UTF-8 text reader. Live Bitrix registration, OS keychain UI,
  actual Codex/Claude sessions and Windows-native terminal behavior unverified.

- Release integration-add prompt verified in a controlling PTY: hidden webhook,
  optional blank display name, no integration ID/token output. An unreachable
  HTTPS fixture failed before credentials/config were saved (expected).
