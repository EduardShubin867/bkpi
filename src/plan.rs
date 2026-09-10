//! Agreed work references, separate from KPI semantics and factual evidence.
use crate::workspace::{Workspace, period, validate_id};
use anyhow::{Context, Result, ensure};
use chrono::{Months, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum PlanSource {
    GoogleDrive,
    LocalFile,
    Url,
    Reference,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanScope {
    Workspace,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanReference {
    pub id: String,
    pub label: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub source: PlanSource,
    pub reference: String,
    pub scope: PlanScope,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub person_sections: BTreeMap<String, String>,
}

/// Reject recognizable credentials before persistence; never echo the supplied value.
pub fn safe_text(text: &str) -> Result<()> {
    let lower = text.to_ascii_lowercase();
    ensure!(
        crate::credentials::redact_text(text) == text
            && ![
                "ya29.",
                "1//",
                "access_token",
                "refresh_token",
                "oauth",
                "authorization:",
                "api_key",
                "sk-or-",
                "client_secret"
            ]
            .iter()
            .any(|v| lower.contains(v)),
        "Credentials are not allowed in plan/scoring data"
    );
    Ok(())
}
impl PlanReference {
    pub fn for_month(
        month: &str,
        reference: String,
        source: Option<PlanSource>,
        label: Option<String>,
    ) -> Result<Self> {
        let (start_date, end_date) = month_dates(month)?;
        let source = source.unwrap_or_else(|| match url::Url::parse(&reference) {
            Ok(url) if matches!(url.host_str(), Some("docs.google.com" | "drive.google.com")) => {
                PlanSource::GoogleDrive
            }
            Ok(url) if matches!(url.scheme(), "http" | "https") => PlanSource::Url,
            _ => PlanSource::LocalFile,
        });
        Ok(Self {
            id: month.into(),
            label: label.unwrap_or_else(|| format!("Plan {month}")),
            start_date,
            end_date,
            source,
            reference,
            scope: PlanScope::Workspace,
            person_sections: BTreeMap::new(),
        })
    }
    fn validate(&self, ws: &Workspace) -> Result<()> {
        validate_id(&self.id)?;
        ensure!(
            self.start_date <= self.end_date,
            "Plan start_date must be before end_date"
        );
        ensure!(
            !self.label.trim().is_empty() && !self.reference.trim().is_empty(),
            "Plan label and reference are required"
        );
        ensure!(
            !self.reference.chars().any(char::is_control),
            "Plan must contain a reference, not document content"
        );
        safe_text(&serde_json::to_string(self)?)?;
        if let Ok(url) = url::Url::parse(&self.reference) {
            ensure!(
                url.username().is_empty() && url.password().is_none() && url.query().is_none(),
                "Plan URL must not contain credentials or query parameters; use a canonical document link"
            );
        }
        if matches!(self.source, PlanSource::GoogleDrive | PlanSource::Url) {
            let url = url::Url::parse(&self.reference)
                .map_err(|_| anyhow::anyhow!("Plan requires an HTTPS URL"))?;
            ensure!(
                url.scheme() == "https" && url.host_str().is_some(),
                "Plan requires an HTTPS URL"
            );
            if self.source == PlanSource::GoogleDrive {
                ensure!(
                    matches!(url.host_str(), Some("docs.google.com" | "drive.google.com")),
                    "Google Drive plan requires docs.google.com or drive.google.com"
                );
                ensure!(
                    url.fragment().is_none_or(|f| f.starts_with("heading=")),
                    "Unsupported Google document fragment"
                );
            } else {
                ensure!(
                    url.fragment().is_none(),
                    "Use a canonical URL without a fragment"
                );
            }
        }
        for (id, section) in &self.person_sections {
            ensure!(
                ws.people.iter().any(|p| &p.id == id),
                "Plan mapping references an unknown workspace person"
            );
            ensure!(
                !section.trim().is_empty(),
                "Plan section identifier cannot be empty"
            );
        }
        Ok(())
    }
}
pub fn month_dates(month: &str) -> Result<(NaiveDate, NaiveDate)> {
    period(Some(month))?;
    let start = NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d")?;
    let end = start
        .checked_add_months(Months::new(1))
        .and_then(|d| d.pred_opt())
        .context("Period out of range")?;
    Ok((start, end))
}
impl Workspace {
    pub fn validate_plans(&self) -> Result<()> {
        let mut ids = BTreeSet::new();
        for plan in &self.plans {
            ensure!(ids.insert(&plan.id), "Duplicate plan ID");
            plan.validate(self)?;
        }
        Ok(())
    }
    pub fn plans_for(&self, month: &str) -> Result<Vec<&PlanReference>> {
        let (start, end) = month_dates(month)?;
        Ok(self
            .plans
            .iter()
            .filter(|p| p.start_date <= end && p.end_date >= start)
            .collect())
    }
    pub fn set_plan(&mut self, plan: PlanReference) -> Result<()> {
        let mut next = self.clone();
        next.plans.retain(|p| p.id != plan.id);
        next.plans.push(plan);
        next.plans.sort_by(|a, b| a.id.cmp(&b.id));
        next.save()?;
        *self = next;
        Ok(())
    }
    pub fn clear_plan(&mut self, id: &str) -> Result<()> {
        ensure!(self.plans.iter().any(|p| p.id == id), "Plan ID not found");
        let mut next = self.clone();
        next.plans.retain(|p| p.id != id);
        next.save()?;
        *self = next;
        Ok(())
    }
}
