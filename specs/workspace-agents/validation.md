# Local validation — 2026-09-08

- cargo fmt --check: passed.
- cargo clippy --all-targets --all-features -- -D warnings: passed, no warnings.
- cargo test: 25 passed.
- cargo test --all-features: 27 passed.
- cargo build --release: passed.
- cargo build --release --features standalone-ai: passed; target/release/bkpi includes optional AI.
- Installer dependency installation: passed, npm audit reported zero vulnerabilities.
- npm run typecheck, npm run lint, npm run build: passed.
- npm test: 7 passed.
- npm pack: installer/bkpi-0.6.0.tgz generated; includes dist, shared skill, both plugin bundles.
- Packed npx --help, update twice, uninstall: passed in a temporary BKPI_INSTALL_DIR using BKPI_RUNTIME_SOURCE. Config retention checked.
- Official skill and Codex plugin validators: passed (uv isolated PyYAML runtime).
- Real child-process stdio initialization, tools/list, local reads/writes: passed through Rust integration tests.
- Mock HTTP tests cover snapshots, selected team, cross-portal partial failure, pagination/cache, bounded retries and secret echo redaction.
- Migration tests verify successful credential movement, original backup, idempotence and failure preserving the original config.

## Verification limits

No real Bitrix/OpenRouter calls, OS credential dialogs or real agent config mutations. No release upload, npm publish or other-OS build executed. Windows/Linux store support is compiled by the prepared release matrix, not proven by this macOS run. The npm installation emitted local allow-scripts policy notices for esbuild/fsevents; no compiler/linter warnings remain and all installer checks pass. No global npm policy was changed.

Git diff is unavailable because the supplied directory has no .git. Original source was archived before changes and the preserved Bitrix implementation compared against that copy. Generated navigation cache was removed after use; no profile assumptions remain in runtime or agent/installer sources.

GitHub release repository must be set in installer/package.json or BKPI_RELEASE_REPOSITORY before using remote downloads. The package name remains subject to npm availability.
