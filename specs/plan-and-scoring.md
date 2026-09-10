# Period Plan and deterministic KPI scoring

BKPI 0.6.x extension. KPI.md defines human business semantics; a Plan defines agreed
work for a period; Bitrix supplies factual evidence; the agent matches these sources;
Rust computes payout from structured assessments. No Google API or LLM inside the
calculator. No built-in Senior profile. Existing Workspace / Person / Integration,
arbitrary KPI paths, multi-person and multi-Bitrix configurations remain unchanged.

## Plan schema and CLI

Optional additions to `.bkpi/workspace.toml` (version remains 1):

```toml
[[plans]]
id = "2026-09"
label = "План разработки — сентябрь 2026"
start_date = "2026-09-01"
end_date = "2026-09-30"
source = "google_drive"
reference = "https://docs.google.com/document/d/DOCUMENT_ID/edit"
scope = "workspace"

# Optional explicit BKPI Person ID -> exact plan section identifier.
[plans.person_sections]
eduard-44843 = "Эдуард Шубин"
```

Sources: google_drive, local_file, url, reference. Scope is workspace, so one document
serves the team without person copies. Dates are inclusive. A monthly query returns
all overlapping references, ordered by persisted ID; it does not infer revision approval.
Conversation/attachment plans are transient agent input and cannot be stored as a source.
The runtime never reads local or remote plan content. `person_sections` is optional;
only workspace Person IDs are accepted. There is no corporate email field added to Person;
agents may use an email only when available from reliable external evidence.

```sh
bkpi --workspace /absolute/project plan set --period 2026-09 \
  --label 'План разработки — сентябрь 2026' \
  'https://docs.google.com/document/d/DOCUMENT_ID/edit'
bkpi --workspace /absolute/project plan list
bkpi --workspace /absolute/project plan show --period 2026-09
bkpi --workspace /absolute/project plan clear --period 2026-09
```

All output is JSON; `--json` is accepted. `set` upserts a month-ID reference; `clear`
removes the specified plan ID (the --period argument), never the source document.
Updating a month reference preserves its explicit person mappings and existing label
unless a new label is supplied. Removing a Person removes only their plan mapping;
the shared plan, other mappings and retained evidence files survive.
Manual TOML may use custom IDs or date ranges. `show` without --period uses the only
plan's starting month when unique, otherwise the current month. Snapshot selection
uses its existing explicit month / person state period / current month resolution.
For custom IDs use `plan list` and the same ID with `clear --period ID`.
There is no interactive plan wizard in this extension; commands are reproducible.

Use canonical HTTPS document links: no URL userinfo or query parameters, credentials,
webhooks, OAuth tokens or document text. Google links allow a heading= fragment.
Google Drive credentials belong solely to the agent connector environment. Config has
no fields for credentials or contents and rejects unknown fields. Recognizable secret
patterns are rejected before persistence; no heuristic can identify every arbitrary
secret string, so do not put secrets into labels, reasoning or references.

## Scoring schema

Optional `/absolute/project/scoring.toml` applies to every Person in that workspace.
No global fallback. No implicit Senior defaults. The complete requested five-criterion
example is [examples/scoring-senior.toml](../examples/scoring-senior.toml).

Minimal valid configuration:

```toml
currency = "RUB"
money_scale = 0
max_payout = 40000
rounding = "half_up_per_criterion"

[time]
enabled = true
mode = "actual_over_planned"
cap = "1"  # default when omitted; allowed range 0..1

[[criteria]]
id = "delivery"
title = "Согласованный технический результат"
max_payout = 40000
input = "progress"

[[criteria.levels]]
id = "full"
min_progress = "0.85"
factor = "1"

[[criteria.levels]]
id = "none"
factor = "0"
```

For categorical input use `input="level"` with arbitrary named levels and factors;
no min_progress allowed. Example levels: full="1", partial="0.5", none="0".
There may be any positive number of criteria and any number of levels. IDs are unique
ASCII IDs, titles are required. Progress selects the greatest satisfied threshold;
exactly one threshold-free fallback is required. Thresholds are unique and within 0..1;
a threshold of zero is allowed (usually the fallback suffices). Factors and time cap must be in 0..1. Sum of criterion maxima
must exactly equal declared maximum. Negative or overflowing amounts are rejected.

Money is an unsigned 64-bit integer in units of 10^-money_scale. Scale 0 means whole
RUB/USD/etc.; scale 2 means kopeks/cents; supported scales are 0..6. Fractional factors,
progress and time use **quoted decimal strings**, up to six fractional digits, or
integers. Unquoted floating-point inputs are rejected rather than approximated. This
is an intentional difference from the sketch schema, making JSON and TOML exact with
no extra dependencies. Output fractions are also decimal strings.

The fixed-scale parser and integer rational arithmetic never use binary floats.
Criterion base = max * factor; final = max * factor * exact time ratio. Each is
independently rounded half-up to one configured money unit, so intermediate displayed
base rounding does not feed final math. Totals sum rounded criterion lines. Rounded
per-line totals can differ from rounding a combined raw total; this is the chosen
explicit policy. No criterion or total can exceed its configured maximum.

Time ratio is min(actual/planned, cap). Planned must be >0 and actual >=0; fractional
days are allowed, up to 1,000,000 days. The displayed factor is rounded half-up to six
places; payout uses the exact ratio, never that displayed decimal. If enabled and any
time input is absent, final amounts are null; there is no silent full-time fallback.
Setting time.enabled=false explicitly disables time adjustment. Known invalid time
inputs are rejected even when adjustment is disabled.

## Request, result, CLI and MCP

Domain models: PlanReference, PlanSource, PlanScope; ScoringConfig, Criterion, Level,
InputKind, TimeConfig, Rounding, Fixed; CalculationRequest, Assessment, Confidence;
CalculationResult, CriterionResult and TimeResult. Workspace owns optional plans and
loads the separate scoring file. `scoring::calculate` is pure; workspace adapters only
resolve Person and read configuration.

```json
{
  "person": "self",
  "period": "2026-09",
  "planned_available_days": 21,
  "actual_worked_days": 20,
  "scenario": "confirmed",
  "assessments": [
    {"criterion_id":"delivery","progress":"0.92","confidence":"confirmed","evidence_references":["task:42"]},
    {"criterion_id":"initiative","level":"full"},
    {"criterion_id":"automation","level":"full"},
    {"criterion_id":"quality","level":"full"},
    {"criterion_id":"knowledge","level":"full"}
  ]
}
```

Only progress/level are mathematical inputs. Evidence references, reasoning, confidence
and scenario are metadata. Agent cannot supply amounts/factors. Unknown IDs/levels,
duplicate assessments, input-type mismatches and extra fields are errors. Missing
assessments or missing progress/level are unresolved, not zero. Confidence unknown does
not choose partial; an explicitly supplied business level remains independent of
confidence. Agent must omit a level it cannot establish from business facts.

```sh
bkpi --workspace /absolute/project scoring show --json
bkpi --workspace /absolute/project calculate --input-json request.json
cat request.json | bkpi --workspace /absolute/project calculate --input-json -
# --json alone reads stdin too.
```

New tools (in addition to existing tools):

- `scoring_get`: optional workspace; returns configured=false or complete validated rules.
- `kpi_calculate`: optional workspace plus request fields above; returns deterministic
  amounts. Pure read-only annotation, no credential access, Bitrix, Drive, LLM or writes.

`bkpi agent call TOOL --args JSON` is the same MCP implementation. Workspace binding
is enforced. JSON errors never echo arbitrary request contents. Fraction schema is
string-or-integer; omit unknown optional values rather than sending null through a host
that validates the JSON schema. Rust deserialization additionally accepts null for
optional fields.

Snapshot keeps the existing `period: "YYYY-MM"` representation and adds period-filtered
`plans` plus `scoring: {configured, currency, money_scale, max_payout, rounding, time,
criteria}`. No-scoring returns configured=false. Old workspace and person schemas
continue to load; no scoring file is created by init or plan commands.

Output contains person/period/scenario, currency/scale/max, time inputs/factor/status,
per-criterion assessment/resolved level/factor/base/final/status, and totals. When any
criterion is unresolved, base_total and final_total are null; separately labeled
resolved_base_subtotal and resolved_final_subtotal summarize resolved lines only.
When time is unknown, base_total may resolve, but all final amounts and final subtotal
are null. Do not present partial subtotals as a full payout.

## Senior examples

| Criterion | Maximum | All full, 21/21 | All full, 20/21 |
|---|---:|---:|---:|
| Delivery, progress >=0.85 | 40000 | 40000 | 38095 |
| Initiative | 50000 | 50000 | 47619 |
| Automation | 20000 | 20000 | 19048 |
| Quality | 20000 | 20000 | 19048 |
| Knowledge | 40000 | 40000 | 38095 |
| Total | 170000 | 170000 | 161905 |

20/21 has displayed factor "0.952381". Delivery 0.84 gives zero; 0.85 gives 40000
before time. Partial levels give 25000/10000/10000/20000 before time. Agreed vacation
example: 21 calendar workdays minus 5 vacation days means planned=16; actual=16 gives
170000, actual=15 gives 159375. Overtime is capped, never a bonus multiplier.

## Agent flows and examples

1. Resolve Person(s), then current-conversation KPI or own configured KPI document.
2. Resolve period and current-conversation plan before workspace PlanReference.
3. Use the compatible Drive connector when available; retrieve only needed content.
4. Scope shared plan to requested workspace people; match explicit mapping, corporate
   email when available, exact full name, then other unambiguous IDs. Ambiguity is a question.
5. Fetch Bitrix evidence and respect coverage limits. Classify planned, replacement,
   urgent-unplanned, unclear; use latest explicitly agreed revision, cite approval.
6. Read scoring rules and build business assessments independently of confidence.
7. Use reliable adjusted planned/actual days, omit unknown values.
8. Call Rust calculator and use returned amounts verbatim. Scenarios are separate calls.

**A. Personal:** Person eduard-44843 has arbitrary KPI.md and a shared Google plan.
The agent finds the exact `Эдуард Шубин` section (or person_sections mapping), uses only
that person's agreed items, and compares them with Bitrix user 44843 evidence. Other
team members in that Google document are excluded from the personal analysis.

**B. Manager:** Many workspace People share one Google team plan. Extract only those
workspace People requested; use each person's own KPI document and separate assessment.
The current implementation shares scoring rules across the workspace; if people's
payout rules differ, use separate workspaces or leave payout unresolved pending a suitable
configuration. Do not apply incompatible common rules silently.

**C. No connector:** Explain the document cannot be read and ask for attach/export/paste.
Do not ask for tokens; pasted content is transient and overrides saved reference.

**D. Exact calculation:** Given the request above and Senior example configuration,
call kpi_calculate; use final_total=161905 RUB verbatim with its per-line breakdown.

**E. No scoring:** Continue qualitative KPI analysis, evidence, risks and questions;
never claim exact deterministic payout. No plan is also backward compatible, but a
criterion requiring agreed scope remains unresolved without that evidence.

Standalone `ai analyze` cannot call the calculator, so with scoring configured it fails
before network and directs users to the agent skill + calculator. Without scoring its
existing optional functionality remains available. Codex/Claude use the same bundled
skill; no Google integration is added to the Rust runtime.

## Add to an existing workspace

The real personal workspace was not inferred or modified. From the BKPI source checkout,
copy the example explicitly to your selected workspace, without overwriting an existing
file:

```sh
cp -n examples/scoring-senior.toml /absolute/project/scoring.toml
bkpi --workspace /absolute/project scoring show --json
bkpi --workspace /absolute/project calculate --input-json /absolute/path/to/bkpi/examples/calculation-senior.json
```

Use the complete example file unchanged for the requested 170k setup; edit only after
checking consistency with the person's actual KPI. The request example uses self,
so configure self or replace it with an existing Person ID. Plan command is shown above.

## Validation and manual boundary

Automated acceptance and edge cases are in tests/plan_scoring.rs and tests/core.rs;
real CLI and stdio MCP subprocesses are exercised with temporary workspaces and no
external credentials. Required gate results are recorded in plan-and-scoring/tasks.md.

Manual follow-up: a real Codex/Claude host must load the updated skill/tools, a Drive
connector must have document access, and the intended Bitrix integration must provide
relevant evidence. Verify actual section matching, revision approval, reporting-period
coverage, KPI/scoring semantic consistency and trustworthy time data in that session.
Mock/subprocess tests do not establish those external facts.
