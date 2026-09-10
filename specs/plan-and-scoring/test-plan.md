# Test plan

Scoring: Senior full=170000, delivery 0.84/0.85, every partial, time 20/21,
overtime cap, zero/negative time, missing time/input, confidence separation,
unknown IDs/levels, duplicate thresholds/IDs, invalid totals/factors, exact rounding,
currency scale, disabled time and repeatability.

Plans: CLI set/show/list/clear, shared workspace scopes, inclusive overlapping periods,
snapshot references, no document content/credential persistence, legacy schema loading,
connector/scoping/precedence instructions in every bundled skill.

MCP: real stdio initialization, schemas, scoring_get and kpi_calculate without any
credential configuration, malformed requests, read-only workspace and binding checks.

Gates: cargo fmt --check; cargo clippy --all-targets --all-features -- -D warnings;
cargo test; cargo test --all-features; cargo build --release. Installer: npm run
typecheck; npm run lint; npm test; npm run build. Release CLI smoke in temporary
workspace. Real Codex/Claude + Drive + Bitrix session remains manual.
