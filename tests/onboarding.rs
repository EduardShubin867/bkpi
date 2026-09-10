use bkpi::{
    config::{Config, Integration, integration_id},
    credentials::{CredentialRef, private_write},
    workspace::{KPI_TEMPLATE, Person, Workspace, input_path},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};
use tempfile::TempDir;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

fn integration(id: &str) -> Integration {
    Integration {
        id: id.into(),
        display_name: "Астро-Волга".into(),
        base_url: "https://portal.test".into(),
        credential: CredentialRef::File {
            account: "fixture".into(),
        },
    }
}
fn person() -> Person {
    Person {
        id: "one".into(),
        name: "Jane Doe".into(),
        integration: "portal".into(),
        bitrix_user_id: "1".into(),
        is_self: true,
        kpi: "KPI.md".into(),
    }
}
fn run(project: &Path, config: &Path, args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bkpi"))
        .args(args)
        .current_dir(project)
        .env("BKPI_CONFIG_DIR", config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("fixture-token"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("fixture-token"));
}
#[test]
fn generated_ids_use_hostname_and_number_collisions() {
    assert_eq!(
        integration_id("https://astrovolga.bitrix24.ru", &[]).unwrap(),
        "astrovolga"
    );
    assert_eq!(
        integration_id("https://WWW.Team.Example.COM", &[]).unwrap(),
        "team-example-com"
    );
    assert_eq!(
        integration_id("https://team.bitrix24.ru.evil.test", &[]).unwrap(),
        "team-bitrix24-ru-evil-test"
    );
    assert_eq!(
        integration_id(
            "https://astrovolga.bitrix24.ru/rest/123/fixture-token/",
            &[integration("astrovolga"), integration("astrovolga-2")]
        )
        .unwrap(),
        "astrovolga-3"
    );
    assert_eq!(
        integration_id("https://astrovolga.bitrix24.ru", &[]).unwrap(),
        integration_id("https://astrovolga.bitrix24.ru/", &[]).unwrap()
    );
    let id = integration_id(
        &format!("https://{}.{}.test", "x".repeat(60), "y".repeat(60)),
        &[],
    )
    .unwrap();
    bkpi::workspace::validate_id(&id).unwrap();
}
#[test]
fn path_input_supports_spaces_quotes_relative_absolute_and_home() {
    let d = TempDir::new().unwrap();
    for input in ["my KPI.md", "'my KPI.md'", "\"my KPI.md\"", "my\\ KPI.md"] {
        assert_eq!(
            input_path(input, d.path()).unwrap(),
            d.path().join("my KPI.md")
        );
    }
    let file = d.path().join("my KPI.md");
    assert_eq!(input_path(&file.to_string_lossy(), d.path()).unwrap(), file);
    assert_eq!(
        input_path("~/Documents/my KPI.md", d.path()).unwrap(),
        dirs::home_dir().unwrap().join("Documents/my KPI.md")
    );
    assert!(input_path("", d.path()).is_err());
}
#[test]
fn visible_and_legacy_documents_load_without_migration() {
    let d = TempDir::new().unwrap();
    let mut ws = Workspace::create(d.path()).unwrap();
    let mut p = person();
    p.kpi = ws.visible_kpi(&p, true);
    ws.add(p).unwrap();
    assert_eq!(
        fs::read_to_string(d.path().join("KPI.md")).unwrap(),
        KPI_TEMPLATE
    );
    assert!(ws.root.join("people/one/state.toml").exists());
    assert!(!ws.root.join("people/one/kpi.md").exists());
    assert!(
        ws.document(ws.resolve(None).unwrap())
            .unwrap()
            .content
            .is_none()
    );
    let mut p = person();
    p.id = "two".into();
    p.is_self = false;
    p.kpi = "people/two/kpi.md".into();
    ws.add(p).unwrap();
    fs::write(
        ws.root.join("people/two/kpi.md"),
        "# KPI\n\nВставьте сюда ваши KPI на текущий период.\n",
    )
    .unwrap();
    let loaded = Workspace::load(&ws.root).unwrap();
    assert!(
        loaded
            .document(loaded.resolve(Some("two")).unwrap())
            .unwrap()
            .content
            .is_none()
    );
    fs::write(ws.root.join("people/two/kpi.md"), "Actual legacy KPI").unwrap();
    assert_eq!(
        loaded
            .document(loaded.resolve(Some("two")).unwrap())
            .unwrap()
            .content
            .as_deref(),
        Some("Actual legacy KPI")
    );
}
#[test]
fn person_kpi_set_show_preserves_files_and_generic_reference() {
    let d = TempDir::new().unwrap();
    let c = TempDir::new().unwrap();
    let external = TempDir::new().unwrap();
    let mut ws = Workspace::create(d.path()).unwrap();
    ws.add(person()).unwrap();
    let show = run(d.path(), c.path(), &["person", "kpi", "show", "--json"], "");
    success(&show);
    let data: Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(data["empty"], true);
    assert_eq!(data["exists"], true);
    let source = external.path().join("custom targets.txt");
    fs::write(&source, "Revenue target").unwrap();
    let set = run(
        d.path(),
        c.path(),
        &["person", "kpi", "set", source.to_str().unwrap(), "--json"],
        "",
    );
    success(&set);
    let data: Value = serde_json::from_slice(&set.stdout).unwrap();
    assert_eq!(data["empty"], false);
    assert_eq!(
        data["path"],
        source.canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(
        fs::read_to_string(d.path().join("KPI.md")).unwrap(),
        KPI_TEMPLATE
    );
    assert_eq!(fs::read_to_string(&source).unwrap(), "Revenue target");
    fs::remove_file(&source).unwrap();
    let show = run(
        d.path(),
        c.path(),
        &["person", "kpi", "show", "one", "--json"],
        "",
    );
    success(&show);
    let data: Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(data["exists"], false);
    assert_eq!(data["empty"], true);
    let old = fs::read(ws.root.join("workspace.toml")).unwrap();
    assert!(
        !run(
            d.path(),
            c.path(),
            &["person", "kpi", "set", "one", "missing.txt"],
            ""
        )
        .status
        .success()
    );
    assert_eq!(fs::read(ws.root.join("workspace.toml")).unwrap(), old);
}
async fn fixture() -> (MockServer, TempDir) {
    let server = MockServer::start().await;
    let me = json!({"ID":"1","NAME":"Jane","LAST_NAME":"Doe"});
    for (method, result) in [
        ("user.current", me.clone()),
        (
            "user.get",
            json!([me,{"ID":"2","NAME":"Jane","LAST_NAME":"Doe"}]),
        ),
    ] {
        Mock::given(path(format!("/rest/1/fixture-token/{method}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"result":result})))
            .mount(&server)
            .await;
    }
    let config = TempDir::new().unwrap();
    Config {
        integrations: vec![integration("portal")],
        ..Default::default()
    }
    .save_at(config.path())
    .unwrap();
    // Local fixture only: no live portal, credentials or changes to TLS policy.
    private_write(
        &config.path().join("secrets/fixture.secret"),
        format!("{}/rest/1/fixture-token", server.uri()).as_bytes(),
    )
    .unwrap();
    (server, config)
}
#[tokio::test(flavor = "multi_thread")]
async fn init_without_file_creates_visible_placeholder_and_precise_summary() {
    let (_server, c) = fixture().await;
    let d = TempDir::new().unwrap();
    let out = run(d.path(), c.path(), &["init"], "y\nme\n\n");
    success(&out);
    assert!(out.stdout.is_empty());
    let text = String::from_utf8(out.stderr).unwrap();
    assert!(text.contains("Do you already have a KPI document?"));
    assert!(text.contains("KPI document is currently empty"));
    assert!(
        text.contains(
            d.path()
                .canonicalize()
                .unwrap()
                .join("KPI.md")
                .to_str()
                .unwrap()
        )
    );
    assert!(!text.contains("Integration ID"));
    assert!(!text.contains("portal-1"));
    assert_eq!(
        fs::read_to_string(d.path().join("KPI.md")).unwrap(),
        KPI_TEMPLATE
    );
}
#[tokio::test(flavor = "multi_thread")]
async fn init_copies_existing_kpi_with_spaces_and_keeps_source() {
    let (_server, c) = fixture().await;
    let d = TempDir::new().unwrap();
    let external = TempDir::new().unwrap();
    let source = external.path().join("my KPI.txt");
    fs::write(&source, "# Sales\nTarget: 100").unwrap();
    let out = run(
        d.path(),
        c.path(),
        &["init"],
        &format!("y\nme\n\"{}\"\n\n", source.display()),
    );
    success(&out);
    assert_eq!(
        fs::read(&source).unwrap(),
        fs::read(d.path().join("KPI.md")).unwrap()
    );
    assert!(
        !String::from_utf8(out.stderr)
            .unwrap()
            .contains("currently empty")
    );
}
#[tokio::test(flavor = "multi_thread")]
async fn init_team_uses_visible_unique_names_and_allows_skips() {
    let (_server, c) = fixture().await;
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("existing.txt"), "Team KPI").unwrap();
    let out = run(
        d.path(),
        c.path(),
        &["init"],
        "y\nsearch\nJane\n1,2\nexisting.txt\ny\n\n",
    );
    success(&out);
    let ws = Workspace::discover(d.path()).unwrap();
    assert_eq!(ws.people[0].kpi, "kpi/jane-doe.md");
    assert_eq!(ws.people[1].kpi, "kpi/jane-doe-2.md");
    assert_eq!(
        fs::read_to_string(d.path().join("kpi/jane-doe.md")).unwrap(),
        "Team KPI"
    );
    assert_eq!(
        fs::read_to_string(d.path().join("kpi/jane-doe-2.md")).unwrap(),
        KPI_TEMPLATE
    );
    assert!(
        !run(d.path(), c.path(), &["person", "kpi", "show", "--json"], "")
            .status
            .success()
    );
    success(&run(
        d.path(),
        c.path(),
        &["person", "kpi", "set", "portal-2", "existing.txt", "--json"],
        "",
    ));
}
#[tokio::test(flavor = "multi_thread")]
async fn init_reference_cancel_and_collision_preserve_user_data() {
    let (_server, c) = fixture().await;
    let external = TempDir::new().unwrap();
    let source = external.path().join("source.md");
    fs::write(&source, "Source").unwrap();
    let d = TempDir::new().unwrap();
    success(&run(
        d.path(),
        c.path(),
        &["init"],
        &format!("y\nme\n{}\nn\n", source.display()),
    ));
    assert!(!d.path().join("KPI.md").exists());
    let ws = Workspace::discover(d.path()).unwrap();
    assert_eq!(
        ws.kpi_path(&ws.people[0]).unwrap(),
        source.canonicalize().unwrap()
    );
    for input in [
        "y\nme\nmissing.md\n".to_string(),
        "y\nme\n".to_string(),
        format!("y\nme\n{}\ny\n", source.display()),
    ] {
        let d = TempDir::new().unwrap();
        fs::write(d.path().join("KPI.md"), "Keep me").unwrap();
        assert!(!run(d.path(), c.path(), &["init"], &input).status.success());
        assert!(!d.path().join(".bkpi").exists());
        assert_eq!(
            fs::read_to_string(d.path().join("KPI.md")).unwrap(),
            "Keep me"
        );
    }
    assert_eq!(fs::read_to_string(source).unwrap(), "Source");
}
#[test]
fn integration_input_errors_never_echo_webhook() {
    let d = TempDir::new().unwrap();
    let c = TempDir::new().unwrap();
    let out = run(
        d.path(),
        c.path(),
        &["integration", "add"],
        "http://portal.test/rest/1/fixture-token/\n",
    );
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let text = String::from_utf8(out.stderr).unwrap();
    assert!(!text.contains("fixture-token"));
    assert!(!text.contains("Integration ID"));
}
#[test]
fn shared_skill_documents_chat_fallback_and_explicit_consent() {
    let text = include_str!("../agent/skill/SKILL.md");
    assert!(text.contains("workspace is not broken"));
    assert!(text.contains("Do not save these KPI automatically"));
    assert!(text.contains("Write only after explicit user consent"));
    assert!(text.contains("allowed local filesystem/workspace operation"));
    assert!(
        text.find("KPI explicitly supplied").unwrap() < text.find("That person's generic").unwrap()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn scripted_person_add_needs_no_new_stdin_and_existing_kpi_is_retained() {
    let (_server, config) = fixture().await;
    let d = TempDir::new().unwrap();
    Workspace::create(d.path()).unwrap();
    fs::write(d.path().join("KPI.md"), "Existing criteria").unwrap();
    success(&run(
        d.path(),
        config.path(),
        &[
            "person",
            "add",
            "--integration",
            "portal",
            "--user-id",
            "1",
            "--name",
            "Эдуард Шубин",
            "--self",
        ],
        "",
    ));
    assert_eq!(
        fs::read_to_string(d.path().join("KPI.md")).unwrap(),
        "Existing criteria"
    );
    success(&run(
        d.path(),
        config.path(),
        &[
            "person",
            "add",
            "--integration",
            "portal",
            "--user-id",
            "2",
            "--name",
            "Иван Иванов",
        ],
        "",
    ));
    assert!(d.path().join("kpi/иван-иванов.md").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn scripted_self_add_does_not_require_kpi_input() {
    let (_server, config) = fixture().await;
    let d = TempDir::new().unwrap();
    Workspace::create(d.path()).unwrap();
    success(&run(
        d.path(),
        config.path(),
        &["person", "add", "--self"],
        "",
    ));
    assert_eq!(
        fs::read_to_string(d.path().join("KPI.md")).unwrap(),
        KPI_TEMPLATE
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn doctor_shows_backend_permissions_and_user_without_secret() {
    let (server, c) = fixture().await;
    Mock::given(path("/rest/1/fixture-token/tasks.task.list"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"result":{"tasks":[]}})))
        .mount(&server)
        .await;
    let d = TempDir::new().unwrap();
    for args in [vec!["doctor"], vec!["integration", "doctor", "portal"]] {
        let out = run(d.path(), c.path(), &args, "");
        success(&out);
        let text = String::from_utf8(out.stderr).unwrap();
        for expected in [
            "credential backend = file",
            "credential configured",
            "Bitrix reachable",
            "user =",
            "task access works",
        ] {
            assert!(text.contains(expected), "{text}");
        }
        #[cfg(unix)]
        assert!(text.contains("credential file permissions = secure"));
    }
}
