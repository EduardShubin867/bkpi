# Requirements

Extend 0.6.x without replacing Workspace/Person/Integration, arbitrary KPI documents,
multi-person/multi-Bitrix, credential backends, MCP or agent installation.

- Optional shared dated PlanReference, reference-only persistence and period selection.
- Conversation plan overrides saved plan; Drive access belongs to agent connector.
- Optional workspace scoring.toml defines arbitrary criteria, thresholds and levels.
- Pure Rust calculator owns money; missing assessment/time remains unresolved.
- Exact arithmetic, bounded factors, validated totals and deterministic rounding.
- CLI, MCP, snapshots and both agent distributions expose the same contract.
- No external writes, Drive runtime dependency, built-in Senior defaults or credentials.

Acceptance: examples and all requested Rust/installer gates plus CLI/MCP subprocess
tests pass; old workspaces continue qualitative analysis.
