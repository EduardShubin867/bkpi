use crate::{
    bitrix::{BitrixClient, Task},
    config::Config,
    workspace::{Person, Workspace, period},
};
use anyhow::{Context, Result, ensure};
use futures::{StreamExt, stream};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotOptions {
    pub month: Option<String>,
    pub limit: Option<usize>,
}
pub struct Core {
    pub workspace: Workspace,
    clients: BTreeMap<String, BitrixClient>,
}
impl Core {
    pub fn new(workspace: Workspace) -> Self {
        Self {
            workspace,
            clients: BTreeMap::new(),
        }
    }
    pub fn with_clients(workspace: Workspace, clients: BTreeMap<String, BitrixClient>) -> Self {
        Self { workspace, clients }
    }
    pub fn prepare(&mut self, people: &[Person]) -> Result<()> {
        let config = Config::load()?;
        for p in people {
            if !self.clients.contains_key(&p.integration) {
                self.clients
                    .insert(p.integration.clone(), config.client(&p.integration)?);
            }
        }
        Ok(())
    }
    pub fn prepare_team(&mut self, people: &[Person]) {
        for person in people {
            let _ = self.prepare(std::slice::from_ref(person));
        }
    }
    pub fn client(&self, p: &Person) -> Result<&BitrixClient> {
        self.clients
            .get(&p.integration)
            .context("Integration client not loaded")
    }
    pub async fn tasks(&self, p: &Person, month: Option<&str>) -> Result<Vec<Task>> {
        let tasks = self.client(p)?.tasks_for_user(&p.bitrix_user_id).await?;
        let mut tasks: Vec<_> = if let Some(month) = month {
            period(Some(month))?;
            let state = self.workspace.state(p)?;
            tasks
                .into_iter()
                .filter(|t| {
                    in_period(t, month)
                        || state
                            .mappings
                            .iter()
                            .any(|m| m.period == month && m.task_id == t.id)
                })
                .collect()
        } else {
            tasks
        };
        tasks.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(tasks)
    }
    pub async fn snapshot(&self, p: &Person, opts: &SnapshotOptions) -> Result<Value> {
        let state = self.workspace.state(p)?;
        let month = period(opts.month.as_deref().or(state.period.as_deref()))?;
        let plans = self.workspace.plans_for(&month)?;
        let scoring = self.workspace.scoring_info()?;
        let limit = opts.limit.unwrap_or(100);
        ensure!((1..=500).contains(&limit), "limit must be 1..500");
        let mut tasks = self.tasks(p, Some(&month)).await?;
        let missing_mapped: Vec<_> = state
            .mappings
            .iter()
            .filter(|m| m.period == month && !tasks.iter().any(|t| t.id == m.task_id))
            .map(|m| m.task_id.clone())
            .collect();
        // Prioritize explicit mappings without dropping unselected IDs silently.
        tasks.sort_by_key(|t| {
            (
                !state
                    .mappings
                    .iter()
                    .any(|m| m.period == month && m.task_id == t.id),
                t.id.clone(),
            )
        });
        let omitted: Vec<_> = tasks.iter().skip(limit).map(|t| t.id.clone()).collect();
        tasks.truncate(limit);
        let client = self.client(p)?;
        let details = stream::iter(tasks.iter().map(|task| async move {
            let (detail, checklist, results) = tokio::join!(
                client.task(&task.id),
                client.checklist_for_task(&task.id),
                client.results_for_task(&task.id)
            );
            (task.id.clone(), detail, checklist, results)
        }))
        .buffer_unordered(4)
        .collect::<Vec<_>>()
        .await;
        let mut task_details = BTreeMap::new();
        let mut checklists = BTreeMap::new();
        let mut task_results = BTreeMap::new();
        let mut errors = BTreeMap::new();
        for (id, detail, checklist, results) in details {
            let mut failed = Vec::new();
            match detail {
                Ok(v) => {
                    task_details.insert(id.clone(), v);
                }
                Err(_) => failed.push("task_details"),
            }
            match checklist {
                Ok(v) => {
                    checklists.insert(id.clone(), v);
                }
                Err(_) => failed.push("checklists"),
            }
            match results {
                Ok(v) => {
                    task_results.insert(id.clone(), v);
                }
                Err(_) => failed.push("task_results"),
            }
            if !failed.is_empty() {
                errors.insert(id, failed);
            }
        }
        client.sanitize(json!({"schema_version":1,"workspace":self.workspace,"person":p,"kpi_document":self.workspace.document(p)?,"period":month,"plans":plans,"scoring":scoring,"tasks":tasks,"task_details":task_details,"checklists":checklists,"task_results":task_results,"local_mappings":state.mappings.iter().filter(|m|m.period==month).collect::<Vec<_>>(),"evidence":state.evidence.iter().filter(|e|e.period==month).collect::<Vec<_>>(),"notes":state.notes,"coverage":{"missing_mapped_task_ids":missing_mapped,"cache_ttl_seconds":15,"omitted_task_ids":omitted,"detail_errors":errors,"complete":omitted.is_empty() && errors.is_empty() && missing_mapped.is_empty(),"selection":"tasks active during period, dated activity or local mapping; current Bitrix state, not historical reconstruction"}}))
    }
    pub async fn team(&self, ids: &[String], opts: &SnapshotOptions) -> Result<Value> {
        let people = self.select_people(ids)?;
        let mut results = stream::iter(people.into_iter().map(|p| async move {
            let value = match self.snapshot(&p, opts).await { Ok(v) => v, Err(_) => json!({"person":p,"error":"Snapshot unavailable; run integration doctor","coverage":{"complete":false}}) }; (p.id, value)
        })).buffer_unordered(4).collect::<Vec<_>>().await;
        results.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(
            json!({"schema_version":1,"snapshots":results.into_iter().map(|(_,v)|v).collect::<Vec<_>>()}),
        )
    }
    pub fn select_people(&self, ids: &[String]) -> Result<Vec<Person>> {
        if ids.is_empty() {
            return Ok(self.workspace.people.clone());
        }
        let mut result = Vec::new();
        for id in ids {
            let p = self.workspace.resolve(Some(id))?;
            if !result.iter().any(|v: &Person| v.id == p.id) {
                result.push(p.clone());
            }
        }
        Ok(result)
    }
}
fn in_period(task: &Task, month: &str) -> bool {
    let dated = [
        &task.created_date,
        &task.changed_date,
        &task.closed_date,
        &task.deadline,
    ]
    .iter()
    .any(|d| d.as_deref().is_some_and(|v| v.starts_with(month)));
    let created_before_end = task
        .created_date
        .as_deref()
        .is_none_or(|v| v.get(..7).is_none_or(|v| v <= month));
    let not_closed_before = task
        .closed_date
        .as_deref()
        .is_none_or(|v| v.get(..7).is_none_or(|v| v >= month));
    dated || (created_before_end && not_closed_before)
}
