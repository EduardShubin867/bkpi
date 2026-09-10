# Design

Workspace schema version remains 1, with serde-default plans. Plans have IDs, labels,
inclusive dates, source, reference, scope=workspace. Optional person mapping is
represented by plan person_sections (BKPI ID -> exact document section identifier).
Monthly selection returns overlapping references, without choosing an approval revision.
Local plan paths are references too: runtime never reads document content.

Scoring is optional project-root scoring.toml. No global or person default.
Money is u64 units, with money_scale=0 (whole currency) or 2 (minor units), up to 6.
Fractions/time use exact decimal strings (or integers), max six fractional places;
no floating-point inputs or money arithmetic. Fixed-scale i128 rational multiplication
uses exact actual/planned, then rounds half-up per criterion; totals sum rounded lines.
Time factor presentation is rounded to six places but never used for money calculation.
Unknown criteria/levels, duplicates and conflicting input types are errors.
Missing business inputs yield null amounts; resolved subtotals are separately labeled.
Evidence confidence is metadata and cannot select a business level.

CLI and MCP load configuration and resolve workspace person, then call the same pure
calculation function. Neither prepares Bitrix clients. Errors never echo input content.
Snapshot keeps existing period string for compatibility and adds plans/scoring.

Assumptions: workspace-level scoring suffices; period stays YYYY-MM; standalone AI
must fail closed when scoring is configured until it can call the calculator.
