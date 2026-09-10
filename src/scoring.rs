//! Pure, currency-agnostic payout calculation. No I/O in `calculate`.
use crate::{
    plan::safe_text,
    workspace::{Workspace, period, validate_id},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
};

const SCALE: i128 = 1_000_000;

/// Exact decimal with six fractional places. Decimal strings avoid lossy JSON/TOML floats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fixed(i128);
impl Fixed {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(SCALE);
    pub fn parse(s: &str) -> Result<Self> {
        ensure!(
            !s.is_empty() && s.len() <= 24,
            "Expected a decimal string with at most six fractional digits"
        );
        let (negative, value) = s.strip_prefix('-').map_or((false, s), |v| (true, v));
        let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
        ensure!(
            !whole.is_empty()
                && whole.bytes().all(|b| b.is_ascii_digit())
                && fraction.len() <= 6
                && fraction.bytes().all(|b| b.is_ascii_digit()),
            "Expected a decimal string with at most six fractional digits"
        );
        let whole: i128 = whole.parse().context("Decimal out of range")?;
        let fractional: i128 = if fraction.is_empty() {
            0
        } else {
            fraction.parse()?
        };
        let value = whole
            .checked_mul(SCALE)
            .and_then(|v| v.checked_add(fractional * 10_i128.pow(6 - fraction.len() as u32)))
            .context("Decimal out of range")?;
        Ok(Self(if negative { -value } else { value }))
    }
}
impl fmt::Display for Fixed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = self.0.abs();
        let fraction = format!("{:06}", value % SCALE);
        let fraction = fraction.trim_end_matches('0');
        write!(f, "{}{}", if self.0 < 0 { "-" } else { "" }, value / SCALE)?;
        if !fraction.is_empty() {
            write!(f, ".{fraction}")?;
        }
        Ok(())
    }
}
impl Serialize for Fixed {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for Fixed {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl de::Visitor<'_> for Visitor {
            type Value = Fixed;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("an exact decimal string (up to six fractional digits) or integer; quote decimal fractions")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Fixed, E> {
                Fixed::parse(v).map_err(E::custom)
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Fixed, E> {
                Ok(Fixed(i128::from(v) * SCALE))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Fixed, E> {
                Ok(Fixed(i128::from(v) * SCALE))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoringConfig {
    pub currency: String,
    /// Amounts are integers in 10^-money_scale currency units.
    #[serde(default)]
    pub money_scale: u32,
    pub max_payout: u64,
    pub rounding: Rounding,
    pub time: TimeConfig,
    pub criteria: Vec<Criterion>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rounding {
    HalfUpPerCriterion,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeConfig {
    pub enabled: bool,
    pub mode: TimeMode,
    #[serde(default = "one")]
    pub cap: Fixed,
}
fn one() -> Fixed {
    Fixed::ONE
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeMode {
    ActualOverPlanned,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    Progress,
    Level,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Criterion {
    pub id: String,
    pub title: String,
    pub max_payout: u64,
    pub input: InputKind,
    pub levels: Vec<Level>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Level {
    pub id: String,
    pub min_progress: Option<Fixed>,
    pub factor: Fixed,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CalculationRequest {
    pub person: String,
    pub period: String,
    pub planned_available_days: Option<Fixed>,
    pub actual_worked_days: Option<Fixed>,
    #[serde(default)]
    pub assessments: Vec<Assessment>,
    pub scenario: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Confirmed,
    Likely,
    Unknown,
    Negative,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assessment {
    pub criterion_id: String,
    pub progress: Option<Fixed>,
    pub level: Option<String>,
    #[serde(default)]
    pub evidence_references: Vec<String>,
    pub confidence: Option<Confidence>,
    pub reasoning: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct TimeResult {
    pub enabled: bool,
    pub planned_available_days: Option<Fixed>,
    pub actual_worked_days: Option<Fixed>,
    /// Presentation only; money uses the exact ratio, never this rounded decimal.
    pub factor: Option<Fixed>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct CriterionResult {
    pub id: String,
    pub title: String,
    pub max_payout: u64,
    pub input: Option<Assessment>,
    pub resolved_level: Option<String>,
    pub factor: Option<Fixed>,
    pub payout_before_time: Option<u64>,
    pub payout_final: Option<u64>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct CalculationResult {
    pub person: String,
    pub period: String,
    pub scenario: Option<String>,
    pub currency: String,
    pub money_scale: u32,
    pub max_payout: u64,
    pub rounding: Rounding,
    pub time: TimeResult,
    pub criteria: Vec<CriterionResult>,
    pub base_total: Option<u64>,
    pub final_total: Option<u64>,
    pub resolved_base_subtotal: u64,
    pub resolved_final_subtotal: Option<u64>,
    pub status: String,
}
impl ScoringConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.currency.trim().is_empty() && self.currency.len() <= 16,
            "Currency must contain 1..16 characters"
        );
        ensure!(self.money_scale <= 6, "money_scale must be 0..6");
        safe_text(&serde_json::to_string(self)?)?;
        unit_interval(self.time.cap, "time cap")?;
        ensure!(
            !self.criteria.is_empty(),
            "At least one criterion is required"
        );
        let mut ids = BTreeSet::new();
        let mut total = 0_u64;
        for c in &self.criteria {
            validate_id(&c.id)?;
            ensure!(ids.insert(&c.id), "Duplicate criterion ID");
            ensure!(!c.title.trim().is_empty(), "Criterion title is required");
            total = total
                .checked_add(c.max_payout)
                .context("Payout total exceeds supported integer range")?;
            ensure!(!c.levels.is_empty(), "Criterion needs levels");
            let mut level_ids = BTreeSet::new();
            let mut thresholds = BTreeSet::new();
            let mut fallback = 0;
            for l in &c.levels {
                validate_id(&l.id)?;
                ensure!(
                    level_ids.insert(&l.id),
                    "Duplicate level ID within criterion"
                );
                unit_interval(l.factor, "level factor")?;
                match (&c.input, l.min_progress) {
                    (InputKind::Level, Some(_)) => {
                        anyhow::bail!("Categorical levels cannot have progress thresholds")
                    }
                    (InputKind::Progress, Some(p)) => {
                        unit_interval(p, "progress threshold")?;
                        ensure!(thresholds.insert(p), "Duplicate progress threshold");
                    }
                    (InputKind::Progress, None) => fallback += 1,
                    _ => {}
                }
            }
            ensure!(
                c.input != InputKind::Progress || fallback == 1,
                "Progress criterion needs exactly one fallback level without min_progress"
            );
        }
        ensure!(
            total == self.max_payout,
            "Sum of criterion max_payout must equal declared max_payout"
        );
        Ok(())
    }
}
fn unit_interval(value: Fixed, field: &str) -> Result<()> {
    ensure!(
        (Fixed::ZERO..=Fixed::ONE).contains(&value),
        "{field} must be between 0 and 1"
    );
    Ok(())
}
/// Positive rational half-up; quotient/remainder avoids addition overflow.
fn rounded(numerator: i128, denominator: i128) -> u64 {
    (numerator / denominator + i128::from(numerator % denominator >= (denominator + 1) / 2)) as u64
}
pub fn calculate(
    config: &ScoringConfig,
    request: &CalculationRequest,
) -> Result<CalculationResult> {
    config.validate()?;
    validate_id(&request.person)?;
    period(Some(&request.period))?;
    safe_text(&serde_json::to_string(request)?)?;
    for (value, planned) in [
        (request.planned_available_days, true),
        (request.actual_worked_days, false),
    ] {
        if let Some(value) = value {
            ensure!(
                value >= Fixed::ZERO && (!planned || value > Fixed::ZERO),
                "planned_available_days must be > 0; actual_worked_days must be >= 0"
            );
            ensure!(
                value.0 <= 1_000_000 * SCALE,
                "Time input must be at most 1000000 days"
            );
        }
    }
    let ratio = if !config.time.enabled {
        Some((1_i128, 1_i128))
    } else {
        request
            .planned_available_days
            .zip(request.actual_worked_days)
            .map(|(planned, actual)| {
                if actual.0 * SCALE >= planned.0 * config.time.cap.0 {
                    (config.time.cap.0, SCALE)
                } else {
                    (actual.0, planned.0)
                }
            })
    };
    let mut inputs = BTreeMap::new();
    for a in &request.assessments {
        let c = config
            .criteria
            .iter()
            .find(|c| c.id == a.criterion_id)
            .context("Unknown criterion ID")?;
        ensure!(
            inputs.insert(&a.criterion_id, a).is_none(),
            "Duplicate assessment criterion ID"
        );
        ensure!(
            !(a.level.is_some() && a.progress.is_some()),
            "Assessment cannot contain both progress and level"
        );
        match c.input {
            InputKind::Progress => {
                ensure!(
                    a.level.is_none(),
                    "Progress criterion does not accept a level"
                );
                if let Some(p) = a.progress {
                    unit_interval(p, "assessment progress")?;
                }
            }
            InputKind::Level => {
                ensure!(
                    a.progress.is_none(),
                    "Categorical criterion does not accept progress"
                );
                if let Some(level) = &a.level {
                    ensure!(c.levels.iter().any(|l| &l.id == level), "Unknown level ID");
                }
            }
        }
    }
    let mut criteria = Vec::new();
    for c in &config.criteria {
        let input = inputs.get(&c.id).copied();
        let selected = input.and_then(|a| match c.input {
            InputKind::Level => a
                .level
                .as_ref()
                .and_then(|id| c.levels.iter().find(|l| &l.id == id)),
            InputKind::Progress => a.progress.and_then(|p| {
                c.levels
                    .iter()
                    .filter(|l| l.min_progress.is_none_or(|t| p >= t))
                    .max_by_key(|l| l.min_progress)
            }),
        });
        let base = selected.map(|l| rounded(i128::from(c.max_payout) * l.factor.0, SCALE));
        let final_amount = selected
            .zip(ratio)
            .map(|(l, (n, d))| rounded(i128::from(c.max_payout) * l.factor.0 * n, SCALE * d));
        criteria.push(CriterionResult {
            id: c.id.clone(),
            title: c.title.clone(),
            max_payout: c.max_payout,
            input: input.cloned(),
            resolved_level: selected.map(|l| l.id.clone()),
            factor: selected.map(|l| l.factor),
            payout_before_time: base,
            payout_final: final_amount,
            status: if selected.is_none() {
                "assessment_unresolved"
            } else if ratio.is_none() {
                "time_unresolved"
            } else {
                "resolved"
            }
            .into(),
        });
    }
    let complete = criteria.iter().all(|c| c.factor.is_some());
    let base: u64 = criteria.iter().filter_map(|c| c.payout_before_time).sum();
    let final_amount: u64 = criteria.iter().filter_map(|c| c.payout_final).sum();
    Ok(CalculationResult {
        person: request.person.clone(),
        period: request.period.clone(),
        scenario: request.scenario.clone(),
        currency: config.currency.clone(),
        money_scale: config.money_scale,
        max_payout: config.max_payout,
        rounding: config.rounding.clone(),
        time: TimeResult {
            enabled: config.time.enabled,
            planned_available_days: request.planned_available_days,
            actual_worked_days: request.actual_worked_days,
            factor: ratio.map(|(n, d)| Fixed(i128::from(rounded(n * SCALE, d)))),
            status: if ratio.is_some() {
                "resolved"
            } else {
                "unresolved"
            }
            .into(),
        },
        criteria,
        base_total: complete.then_some(base),
        final_total: (complete && ratio.is_some()).then_some(final_amount),
        resolved_base_subtotal: base,
        resolved_final_subtotal: ratio.map(|_| final_amount),
        status: if complete && ratio.is_some() {
            "resolved"
        } else {
            "unresolved"
        }
        .into(),
    })
}
impl Workspace {
    pub fn scoring(&self) -> Result<Option<ScoringConfig>> {
        let path = self
            .root
            .parent()
            .context("Workspace project missing")?
            .join("scoring.toml");
        ensure!(!path.is_symlink(), "scoring.toml must not be a symlink");
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => anyhow::bail!("Cannot read scoring.toml"),
        };
        safe_text(&text)?;
        let config: ScoringConfig = toml::from_str(&text).map_err(|_| anyhow::anyhow!("Invalid scoring.toml: check required fields, nonnegative integer payouts and quoted decimal fractions"))?;
        config.validate()?;
        Ok(Some(config))
    }
    pub fn scoring_info(&self) -> Result<serde_json::Value> {
        match self.scoring()? {
            Some(config) => {
                let mut value = serde_json::to_value(config)?;
                value["configured"] = true.into();
                Ok(value)
            }
            None => Ok(serde_json::json!({"configured":false})),
        }
    }
    pub fn calculate(&self, mut request: CalculationRequest) -> Result<CalculationResult> {
        request.person = self.resolve(Some(&request.person))?.id.clone();
        calculate(
            &self
                .scoring()?
                .context("No scoring.toml: qualitative analysis only")?,
            &request,
        )
    }
}
