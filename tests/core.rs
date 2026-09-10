use bkpi::{
    bitrix::BitrixClient,
    config::{Config, Integration, migrate},
    credentials::{CredentialRef, CredentialStore, Secret, private_write, webhook},
    snapshot::{Core, SnapshotOptions},
    workspace::{KPI_TEMPLATE, LocalState, Person, Workspace},
};
use serde_json::json;
use std::{collections::BTreeMap, fs, sync::Mutex};
use tempfile::TempDir;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};
fn person(id: &str, is_self: bool) -> Person {
    Person {
        id: id.into(),
        name: id.into(),
        integration: "portal".into(),
        bitrix_user_id: "1".into(),
        is_self,
        kpi: format!("people/{id}/kpi.md"),
    }
}
fn workspace() -> (TempDir, Workspace) {
    let dir = TempDir::new().unwrap();
    let mut ws = Workspace::create(dir.path()).unwrap();
    ws.add(person("one", true)).unwrap();
    (dir, ws)
}
#[test]
fn parsing_resolution_and_self() {
    let (_d, mut ws) = workspace();
    ws.add(person("two", false)).unwrap();
    let loaded = Workspace::load(&ws.root).unwrap();
    assert_eq!(loaded.resolve(None).unwrap().id, "one");
    assert_eq!(loaded.resolve(Some("self")).unwrap().id, "one");
    assert_eq!(loaded.resolve(Some("two")).unwrap().id, "two");
    assert!(loaded.resolve(Some("missing")).is_err());
    ws.people[0].is_self = false;
    assert!(ws.resolve(None).is_err());
    assert!(ws.resolve(Some("self")).is_err());
    ws.people.pop();
    assert_eq!(ws.resolve(None).unwrap().id, "one");
}
#[test]
fn duplicate_self_and_ids_rejected() {
    let (_d, mut ws) = workspace();
    assert!(ws.add(person("two", true)).is_err());
    assert!(ws.add(person("one", false)).is_err());
    assert_eq!(ws.people.len(), 1);
}
#[test]
fn arbitrary_kpi_and_no_kpi() {
    let (_d, ws) = workspace();
    let p = ws.resolve(None).unwrap();
    assert_eq!(ws.document(p).unwrap().status, "missing");
    private_write(
        &ws.root.join(&p.kpi),
        "# Любые KPI\nТочность прогноза".as_bytes(),
    )
    .unwrap();
    assert!(
        ws.document(p)
            .unwrap()
            .content
            .unwrap()
            .contains("Точность")
    );
    fs::remove_file(ws.root.join(&p.kpi)).unwrap();
    assert_eq!(ws.document(p).unwrap().status, "missing");
}
#[test]
fn paths_and_schema_versions() {
    let (_d, mut ws) = workspace();
    for p in ["../outside", "/tmp/a", "people/../a"] {
        assert!(ws.safe_path(p).is_err());
    }
    ws.version = 2;
    assert!(ws.validate().is_err());
}
#[cfg(unix)]
#[test]
fn symlinks_rejected() {
    use std::os::unix::fs::symlink;
    let (_d, ws) = workspace();
    let outside = TempDir::new().unwrap();
    symlink(outside.path(), ws.root.join("escape")).unwrap();
    assert!(ws.safe_path("escape/file").is_err());
}
#[test]
fn state_roundtrip_and_no_default_criteria() {
    let (_d, ws) = workspace();
    let p = ws.resolve(None).unwrap();
    let state = ws.state(p).unwrap();
    assert!(state.mappings.is_empty() && state.evidence.is_empty());
    ws.update_state(p, |s| {
        s.notes.push("note".into());
        s.period = Some("2026-09".into());
        Ok(())
    })
    .unwrap();
    assert_eq!(ws.state(p).unwrap().notes, vec!["note"]);
    assert!(
        !fs::read_to_string(ws.root.join(&p.kpi))
            .unwrap()
            .contains("Senior")
    );
    assert_eq!(
        fs::read_to_string(ws.root.join(&p.kpi)).unwrap(),
        KPI_TEMPLATE
    );
}
#[test]
fn credential_validation_redaction() {
    let (s, origin) = webhook("https://portal.test/rest/123/private-token/").unwrap();
    assert_eq!(origin, "https://portal.test");
    let safe =
        s.redact("https://portal.test/rest/123/private-token/tasks.task.list and private-token");
    assert!(!safe.contains("private-token"));
    for value in [
        "http://portal.test/rest/1/token",
        "https://portal.test/rest/1/token?key=secret",
        "https://me:pass@portal.test/rest/1/token",
        "nonsense",
    ] {
        assert!(webhook(value).is_err());
    }
}
#[derive(Default)]
struct MemoryStore(Mutex<BTreeMap<String, String>>);
impl CredentialStore for MemoryStore {
    fn save(&self, a: &str, s: &Secret, _: bool) -> anyhow::Result<CredentialRef> {
        self.0.lock().unwrap().insert(a.into(), s.expose().into());
        Ok(CredentialRef::Keyring { account: a.into() })
    }
    fn load(&self, r: &CredentialRef) -> anyhow::Result<Secret> {
        let a = match r {
            CredentialRef::Keyring { account }
            | CredentialRef::File { account }
            | CredentialRef::Env { account } => account,
        };
        Ok(Secret::new(self.0.lock().unwrap()[a].clone()))
    }
    fn remove(&self, _: &CredentialRef) -> anyhow::Result<()> {
        Ok(())
    }
}
#[test]
fn migration_preserves_original_and_credentials() {
    let d = TempDir::new().unwrap();
    let old = "[bitrix]\nwebhook_url='https://portal.test/rest/1/secret/'\n[kpi]\nprofile='mentor_senior'\n[openrouter]\napi_key='sk-or-private'\nmodel='example/model'\n";
    private_write(&d.path().join("config.toml"), old.as_bytes()).unwrap();
    let store = MemoryStore::default();
    assert!(migrate(d.path(), &store, false).unwrap());
    assert_eq!(
        fs::read_to_string(d.path().join("config.legacy-backup.toml")).unwrap(),
        old
    );
    let current = fs::read_to_string(d.path().join("config.toml")).unwrap();
    assert!(
        !current.contains("sk-or-private")
            && !current.contains("/secret/")
            && !current.contains("mentor_senior")
    );
    let c = Config::load_at(d.path()).unwrap();
    assert_eq!(
        store.load(&c.integrations[0].credential).unwrap().expose(),
        "https://portal.test/rest/1/secret/"
    );
    assert!(c.openrouter.is_some());
    assert!(!migrate(d.path(), &store, false).unwrap());
}
#[test]
fn multiple_integrations_roundtrip() {
    let d = TempDir::new().unwrap();
    let c = Config {
        integrations: ["a", "b"]
            .iter()
            .map(|id| Integration {
                id: (*id).into(),
                display_name: (*id).into(),
                base_url: format!("https://{id}.test"),
                credential: CredentialRef::Keyring {
                    account: format!("bitrix-{id}"),
                },
            })
            .collect(),
        ..Default::default()
    };
    c.save_at(d.path()).unwrap();
    let loaded = Config::load_at(d.path()).unwrap();
    assert_eq!(loaded.integration("b").unwrap().base_url, "https://b.test");
    assert!(loaded.integration("c").is_err());
}
async fn mocks(server: &MockServer) {
    let task = json!({"id":"42","title":"Task private-token","responsibleId":"1","status":"3","createdDate":"2026-09-01T10:00:00Z"});
    for (route, value) in [
        ("tasks.task.list", json!({"tasks":[task.clone()]})),
        ("tasks.task.get", json!({"task":task})),
        ("task.checklistitem.getlist", json!([])),
        ("tasks.task.result.list", json!([{"text":"Evidence"}])),
    ] {
        Mock::given(method("POST"))
            .and(path(format!("/rest/1/private-token/{route}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"result":value})))
            .mount(server)
            .await;
    }
}
#[tokio::test]
async fn snapshot_team_redaction_and_json_stability() {
    let (_d, mut ws) = workspace();
    ws.add(person("two", false)).unwrap();
    ws.set_plan(
        bkpi::plan::PlanReference::for_month(
            "2026-09",
            "https://docs.google.com/document/d/team/edit".into(),
            None,
            None,
        )
        .unwrap(),
    )
    .unwrap();
    std::fs::write(
        ws.root.parent().unwrap().join("scoring.toml"),
        include_str!("../examples/scoring-senior.toml"),
    )
    .unwrap();
    let server = MockServer::start().await;
    mocks(&server).await;
    let client = BitrixClient::new(&format!("{}/rest/1/private-token", server.uri())).unwrap();
    let core = Core::with_clients(ws, BTreeMap::from([("portal".into(), client)]));
    let opts = SnapshotOptions {
        month: Some("2026-09".into()),
        limit: Some(100),
    };
    let p = core.workspace.resolve(Some("one")).unwrap();
    let a = core.snapshot(p, &opts).await.unwrap();
    let b = core.snapshot(p, &opts).await.unwrap();
    assert_eq!(a, b);
    assert_eq!(a["kpi_document"]["status"], "missing");
    assert_eq!(
        a["plans"][0]["reference"],
        "https://docs.google.com/document/d/team/edit"
    );
    assert!(a["plans"][0].get("content").is_none());
    assert_eq!(a["scoring"]["configured"], true);
    assert_eq!(a["scoring"]["criteria"].as_array().unwrap().len(), 5);
    assert_eq!(a["coverage"]["complete"], true);
    assert!(!a.to_string().contains("private-token"));
    assert!(a["task_details"]["42"].is_object());
    let team = core.team(&["two".into()], &opts).await.unwrap();
    assert_eq!(team["snapshots"].as_array().unwrap().len(), 1);
    assert_eq!(team["snapshots"][0]["person"]["id"], "two");
    assert_eq!(team["snapshots"][0]["plans"], a["plans"]);
}
#[tokio::test]
async fn errors_never_echo_secrets() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_string("private-token and https://portal/rest/1/private-token"),
        )
        .mount(&server)
        .await;
    let c = BitrixClient::new(&format!("{}/rest/1/private-token", server.uri())).unwrap();
    let err = format!("{:#}", c.current_user().await.unwrap_err());
    assert!(!err.contains("private-token"));
}
#[tokio::test]
async fn partial_details_not_silently_empty() {
    let (_d, ws) = workspace();
    let server = MockServer::start().await;
    Mock::given(path("/tasks.task.list"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"result":{"tasks":[{"id":"1","title":"test"}]}})),
        )
        .mount(&server)
        .await;
    let core = Core::with_clients(
        ws,
        BTreeMap::from([("portal".into(), BitrixClient::new(&server.uri()).unwrap())]),
    );
    let value = core
        .snapshot(
            core.workspace.resolve(None).unwrap(),
            &SnapshotOptions::default(),
        )
        .await
        .unwrap();
    assert_eq!(value["coverage"]["complete"], false);
    assert_eq!(
        value["coverage"]["detail_errors"]["1"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
}
#[tokio::test]
async fn local_tools_set_remove_and_validate() {
    let (_d, ws) = workspace();
    let base = json!({"workspace":ws.root,"person":"one","task_id":"12","criterion":"arbitrary KPI","period":"2026-09","note":"saved"});
    bkpi::mcp::call("bkpi_mapping_set", base.clone(), None)
        .await
        .unwrap();
    bkpi::mcp::call("bkpi_mapping_set", base.clone(), None)
        .await
        .unwrap();
    assert_eq!(
        ws.state(ws.resolve(None).unwrap()).unwrap().mappings.len(),
        1
    );
    let mut remove = base;
    remove.as_object_mut().unwrap().remove("note");
    bkpi::mcp::call("bkpi_mapping_remove", remove, None)
        .await
        .unwrap();
    let e = json!({"workspace":ws.root,"id":"proof","criterion":"anything","period":"2026-09","level":"likely","source":"task:12","note":"needs acceptance"});
    bkpi::mcp::call("bkpi_evidence_set", e, None).await.unwrap();
    assert_eq!(
        ws.state(ws.resolve(None).unwrap()).unwrap().evidence.len(),
        1
    );
    bkpi::mcp::call(
        "bkpi_evidence_remove",
        json!({"workspace":ws.root,"id":"proof"}),
        None,
    )
    .await
    .unwrap();
    assert!(
        ws.state(ws.resolve(None).unwrap())
            .unwrap()
            .evidence
            .is_empty()
    );
    assert!(
        bkpi::mcp::call(
            "bkpi_mapping_set",
            json!({"workspace":ws.root,"unexpected":true}),
            None
        )
        .await
        .is_err()
    );
}
#[test]
fn tool_schemas_are_closed_and_readonly_by_default() {
    let schema = bkpi::mcp::schemas();
    let tools = schema["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 16);
    assert_eq!(
        tools
            .iter()
            .filter(|v| v["annotations"]["readOnlyHint"] == true)
            .count(),
        12
    );
    for t in tools {
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        assert!(!t.to_string().contains("webhook"));
    }
}
#[test]
fn local_state_schema_rejects_unknown() {
    assert!(toml::from_str::<LocalState>("version=1\nwebhook='x'").is_err());
}

#[tokio::test]
async fn pagination_and_cache_preserve_all_tasks() {
    use wiremock::matchers::body_partial_json;
    let server = MockServer::start().await;
    let tasks: Vec<_> = (1..=50)
        .map(|i| json!({"id":i.to_string(),"title":"task"}))
        .collect();
    Mock::given(path("/tasks.task.list"))
        .and(body_partial_json(json!({"start":0})))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"result":{"tasks":tasks},"next":50})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/tasks.task.list"))
        .and(body_partial_json(json!({"start":50})))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"result":{"tasks":[{"id":"51","title":"last"}]}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client = BitrixClient::new(&server.uri()).unwrap();
    assert_eq!(client.tasks_for_user("1").await.unwrap().len(), 51);
    assert_eq!(client.tasks_for_user("1").await.unwrap().len(), 51);
}
#[tokio::test]
async fn transient_failure_retries_are_bounded() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503).set_body_string("never echo private-token"))
        .expect(3)
        .mount(&server)
        .await;
    let client = BitrixClient::new(&server.uri()).unwrap();
    assert!(client.current_user().await.is_err());
}
#[tokio::test]
async fn team_retains_success_when_another_portal_fails() {
    let (_d, mut ws) = workspace();
    let mut second = person("two", false);
    second.integration = "unavailable".into();
    ws.add(second).unwrap();
    let server = MockServer::start().await;
    mocks(&server).await;
    let core = Core::with_clients(
        ws,
        BTreeMap::from([(
            "portal".into(),
            BitrixClient::new(&format!("{}/rest/1/private-token", server.uri())).unwrap(),
        )]),
    );
    let result = core.team(&[], &SnapshotOptions::default()).await.unwrap();
    assert!(result["snapshots"][0]["tasks"].is_array());
    assert_eq!(result["snapshots"][1]["coverage"]["complete"], false);
}
#[tokio::test]
async fn pasted_credentials_are_redacted_without_store_access() {
    let (_d, ws) = workspace();
    let p = ws.resolve(None).unwrap();
    private_write(
        &ws.root.join(&p.kpi),
        b"KPI https://other.test/rest/9/another-secret/ and sk-or-abc123",
    )
    .unwrap();
    let value = bkpi::mcp::call("bkpi_kpi_document_get", json!({"workspace":ws.root}), None)
        .await
        .unwrap();
    assert!(!value.to_string().contains("another-secret"));
    assert!(!value.to_string().contains("sk-or-abc123"));
}
#[cfg(unix)]
#[test]
fn fallback_file_is_private_and_rejects_relaxed_permissions() {
    use bkpi::credentials::SystemStore;
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    let store = SystemStore {
        root: dir.path().into(),
    };
    let r = store
        .save("test", &Secret::new("private".into()), true)
        .unwrap();
    let file = dir.path().join("secrets/test.secret");
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(store.load(&r).unwrap().expose(), "private");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(store.load(&r).is_err());
    store.remove(&r).unwrap();
}
#[test]
fn failed_migration_keeps_legacy_config() {
    struct Failing;
    impl CredentialStore for Failing {
        fn save(&self, _: &str, _: &Secret, _: bool) -> anyhow::Result<CredentialRef> {
            anyhow::bail!("unavailable")
        }
        fn load(&self, _: &CredentialRef) -> anyhow::Result<Secret> {
            anyhow::bail!("unavailable")
        }
        fn remove(&self, _: &CredentialRef) -> anyhow::Result<()> {
            Ok(())
        }
    }
    let dir = TempDir::new().unwrap();
    let old = "[bitrix]\nwebhook_url='https://portal.test/rest/1/secret/'\n";
    private_write(&dir.path().join("config.toml"), old.as_bytes()).unwrap();
    assert!(migrate(dir.path(), &Failing, false).is_err());
    assert_eq!(
        fs::read_to_string(dir.path().join("config.toml")).unwrap(),
        old
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("config.legacy-backup.toml")).unwrap(),
        old
    );
}
