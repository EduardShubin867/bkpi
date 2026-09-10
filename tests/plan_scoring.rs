use bkpi::{
    plan::{PlanReference, PlanSource},
    scoring::{CalculationRequest, Fixed, ScoringConfig, calculate},
    workspace::{Person, Workspace},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};

fn config() -> ScoringConfig {
    toml::from_str(include_str!("../examples/scoring-senior.toml")).unwrap()
}
fn request() -> CalculationRequest {
    serde_json::from_str(include_str!("../examples/calculation-senior.json")).unwrap()
}
fn fixed(s: &str) -> Fixed {
    Fixed::parse(s).unwrap()
}
fn workspace() -> (tempfile::TempDir, Workspace) {
    let d = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(d.path()).unwrap();
    for (id, name) in [("one", "Эдуард Шубин"), ("two", "Иван Иванов")] {
        ws.add(Person {
            id: id.into(),
            name: name.into(),
            integration: "not-configured".into(),
            bitrix_user_id: if id == "one" { "44843" } else { "2" }.into(),
            is_self: id == "one",
            kpi: format!("people/{id}/kpi.md"),
        })
        .unwrap();
    }
    (d, ws)
}
fn install_scoring(ws: &Workspace) {
    fs::write(
        ws.root.parent().unwrap().join("scoring.toml"),
        include_str!("../examples/scoring-senior.toml"),
    )
    .unwrap();
}
fn cli(ws: &Workspace, args: &[&str], input: Option<&Value>) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bkpi"))
        .arg("--workspace")
        .arg(ws.root.parent().unwrap())
        .args(args)
        .env("BKPI_CONFIG_DIR", ws.root.join("unused-global-config"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.to_string().as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}
fn success(out: std::process::Output) -> Value {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn senior_full_threshold_and_each_partial() {
    let c = config();
    let mut r = request();
    assert_eq!(calculate(&c, &r).unwrap().final_total, Some(170000));
    for (progress, expected) in [
        ("0", 0),
        ("0.84", 0),
        ("0.849999", 0),
        ("0.85", 40000),
        ("1", 40000),
    ] {
        r.assessments[0].progress = Some(fixed(progress));
        assert_eq!(
            calculate(&c, &r).unwrap().criteria[0].payout_before_time,
            Some(expected)
        );
    }
    for (index, expected) in [(1, 25000), (2, 10000), (3, 10000), (4, 20000)] {
        let mut r = request();
        r.assessments[index].level = Some("partial".into());
        assert_eq!(
            calculate(&c, &r).unwrap().criteria[index].payout_before_time,
            Some(expected)
        );
    }
}
#[test]
fn time_exact_ratio_overtime_and_vacation() {
    let c = config();
    let mut r = request();
    r.actual_worked_days = Some(fixed("20"));
    let output = calculate(&c, &r).unwrap();
    assert_eq!(output.time.factor.unwrap().to_string(), "0.952381");
    assert_eq!(output.final_total, Some(161905));
    assert_eq!(
        output
            .criteria
            .iter()
            .map(|c| c.payout_final.unwrap())
            .collect::<Vec<_>>(),
        [38095, 47619, 19048, 19048, 38095]
    );
    for (planned, actual, expected) in [
        ("21", "25", 170000),
        ("16", "16", 170000),
        ("16", "15", 159375),
        ("16", "0", 0),
        ("16", "15.5", 164688),
    ] {
        r.planned_available_days = Some(fixed(planned));
        r.actual_worked_days = Some(fixed(actual));
        assert_eq!(calculate(&c, &r).unwrap().final_total, Some(expected));
    }
}
#[test]
fn missing_assessment_and_confidence_never_select_partial() {
    let c = config();
    let mut r = request();
    r.assessments[1].level = None;
    r.assessments[1].confidence = Some(bkpi::scoring::Confidence::Unknown);
    let out = calculate(&c, &r).unwrap();
    assert!(out.criteria[1].factor.is_none());
    assert!(out.criteria[1].payout_before_time.is_none());
    assert!(out.base_total.is_none() && out.final_total.is_none());
    assert_eq!(out.resolved_base_subtotal, 120000);
    r.assessments.remove(1);
    assert_eq!(
        calculate(&c, &r).unwrap().criteria[1].status,
        "assessment_unresolved"
    );
    // Explicitly selected business levels remain independent from confidence metadata.
    r.assessments[1].confidence = Some(bkpi::scoring::Confidence::Unknown);
    assert_eq!(
        calculate(&c, &r).unwrap().criteria[2].factor,
        Some(Fixed::ONE)
    );
}
#[test]
fn missing_time_keeps_base_and_disabled_time_is_explicit() {
    let mut c = config();
    let mut r = request();
    for planned in [None, Some(fixed("21"))] {
        r.planned_available_days = planned;
        r.actual_worked_days = None;
        let out = calculate(&c, &r).unwrap();
        assert_eq!(out.base_total, Some(170000));
        assert!(out.final_total.is_none() && out.time.factor.is_none());
        assert!(out.criteria.iter().all(|c| c.payout_final.is_none()));
    }
    c.time.enabled = false;
    assert_eq!(calculate(&c, &r).unwrap().final_total, Some(170000));
}
#[test]
fn invalid_requests_are_rejected() {
    let c = config();
    for (planned, actual) in [
        ("0", "0"),
        ("-1", "1"),
        ("21", "-1"),
        ("1000001", "1"),
        ("1", "1000001"),
    ] {
        let mut r = request();
        r.planned_available_days = Some(fixed(planned));
        r.actual_worked_days = Some(fixed(actual));
        assert!(calculate(&c, &r).is_err());
    }
    let mut r = request();
    r.assessments[1].level = Some("unknown".into());
    assert!(calculate(&c, &r).is_err());
    let mut r = request();
    r.assessments[1].criterion_id = "missing".into();
    assert!(calculate(&c, &r).is_err());
    let mut r = request();
    r.assessments.push(r.assessments[0].clone());
    assert!(calculate(&c, &r).is_err());
    let mut r = request();
    r.assessments[1].progress = Some(fixed("0.5"));
    assert!(calculate(&c, &r).is_err());
    let mut r = request();
    r.assessments[0].level = Some("full".into());
    assert!(calculate(&c, &r).is_err());
    for progress in ["-0.1", "1.000001"] {
        let mut r = request();
        r.assessments[0].progress = Some(fixed(progress));
        assert!(calculate(&c, &r).is_err());
    }
    let mut r = request();
    r.period = "2026-13".into();
    assert!(calculate(&c, &r).is_err());
    for forbidden in [json!({"max_payout":42}), json!({"factor":"0.5"})] {
        let mut r = serde_json::to_value(request()).unwrap();
        r["assessments"][0]
            .as_object_mut()
            .unwrap()
            .extend(forbidden.as_object().unwrap().clone());
        assert!(serde_json::from_value::<CalculationRequest>(r).is_err());
    }
}
#[test]
fn config_validation_is_generic_and_strict() {
    let mut c = config();
    c.criteria[1].id = c.criteria[0].id.clone();
    assert!(c.validate().is_err());
    let mut c = config();
    c.criteria[1].levels[1].id = "full".into();
    assert!(c.validate().is_err());
    let mut c = config();
    c.max_payout += 1;
    assert!(c.validate().is_err());
    let mut c = config();
    let duplicate = c.criteria[0].levels[0].clone();
    c.criteria[0].levels.push(duplicate);
    c.criteria[0].levels[2].id = "another".into();
    assert!(c.validate().is_err());
    let mut c = config();
    c.criteria[0].levels.pop();
    assert!(c.validate().is_err());
    let mut c = config();
    c.criteria[0].levels[0].min_progress = Some(fixed("1.01"));
    assert!(c.validate().is_err());
    let mut c = config();
    c.criteria[1].levels[0].min_progress = Some(fixed("0.5"));
    assert!(c.validate().is_err());
    for f in ["-0.1", "1.1"] {
        let mut c = config();
        c.criteria[1].levels[0].factor = fixed(f);
        assert!(c.validate().is_err());
        let mut c = config();
        c.time.cap = fixed(f);
        assert!(c.validate().is_err());
    }
    let text = include_str!("../examples/scoring-senior.toml")
        .replace("max_payout = 40000", "max_payout = -1");
    assert!(toml::from_str::<ScoringConfig>(&text).is_err());
}
#[test]
fn exact_rounding_currency_scale_and_large_values() {
    let mut c = config();
    c.criteria.truncate(1);
    c.criteria[0].max_payout = 1;
    c.max_payout = 1;
    let mut r = request();
    r.assessments.truncate(1);
    r.planned_available_days = Some(fixed("2"));
    r.actual_worked_days = Some(fixed("1"));
    c.money_scale = 2; // One cent, half-up to one cent.
    let first = calculate(&c, &r).unwrap();
    assert_eq!(first.final_total, Some(1));
    for _ in 0..100 {
        assert_eq!(
            serde_json::to_value(calculate(&c, &r).unwrap()).unwrap(),
            serde_json::to_value(&first).unwrap()
        );
    }
    c.criteria[0].max_payout = u64::MAX;
    c.max_payout = u64::MAX;
    r.planned_available_days = Some(fixed("1000000"));
    r.actual_worked_days = Some(fixed("1000000"));
    assert_eq!(calculate(&c, &r).unwrap().final_total, Some(u64::MAX));
    c.criteria[0].max_payout = 3;
    c.max_payout = 3;
    c.criteria[0].levels[0].factor = fixed("0.5");
    r.planned_available_days = Some(fixed("2"));
    r.actual_worked_days = Some(fixed("1"));
    assert_eq!(
        calculate(&c, &r).unwrap().criteria[0].payout_before_time,
        Some(2)
    );
    assert_eq!(calculate(&c, &r).unwrap().final_total, Some(1));
    for s in ["NaN", "inf", "1e-2", "0.1234567", ".5", ""] {
        assert!(Fixed::parse(s).is_err());
    }
    assert_eq!(fixed("0.100000").to_string(), "0.1");
    assert!(serde_json::from_str::<Fixed>("0.85").is_err());
}
#[test]
fn configurable_cap_and_arbitrary_criterion_count() {
    let mut c = config();
    c.time.cap = fixed("0.8");
    assert_eq!(calculate(&c, &request()).unwrap().final_total, Some(136000));
    let mut c = config();
    let mut extra = c.criteria[1].clone();
    extra.id = "custom".into();
    extra.max_payout = 123;
    c.max_payout += 123;
    c.criteria.push(extra);
    let mut r = request();
    let mut extra = r.assessments[1].clone();
    extra.criterion_id = "custom".into();
    r.assessments.push(extra);
    assert_eq!(calculate(&c, &r).unwrap().final_total, Some(170123));
}
#[test]
fn plans_cli_roundtrip_and_shared_reference_only() {
    let (_d, ws) = workspace();
    let url = "https://docs.google.com/document/d/example/edit";
    let out = success(cli(&ws, &["plan", "set", "--period", "2026-09", url], None));
    assert_eq!(out["plans"][0]["source"], "google_drive");
    assert_eq!(out["plans"][0]["scope"], "workspace");
    assert_eq!(
        success(cli(&ws, &["plan", "show"], None))["plans"][0]["reference"],
        url
    );
    assert_eq!(
        success(cli(&ws, &["plan", "list"], None))["plans"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        success(cli(&ws, &["plan", "show", "--period", "2026-10"], None))["plans"],
        json!([])
    );
    let loaded = Workspace::load(&ws.root).unwrap();
    assert_eq!(loaded.people.len(), 2);
    assert_eq!(loaded.plans.len(), 1);
    assert_eq!(loaded.plans[0].end_date.to_string(), "2026-09-30");
    let text = fs::read_to_string(ws.root.join("workspace.toml")).unwrap();
    assert_eq!(text.matches(url).count(), 1);
    assert_eq!(
        success(cli(&ws, &["plan", "clear", "--period", "2026-09"], None))["plans"],
        json!([])
    );
}
#[test]
fn plan_dates_mappings_and_no_content_reads() {
    let (d, mut ws) = workspace();
    let file = d.path().join("team.txt");
    fs::write(&file, "PRIVATE DOCUMENT CONTENT").unwrap();
    let mut p = PlanReference::for_month(
        "2024-02",
        file.display().to_string(),
        Some(PlanSource::LocalFile),
        None,
    )
    .unwrap();
    assert_eq!(p.end_date.to_string(), "2024-02-29");
    p.person_sections
        .insert("one".into(), "Эдуард Шубин".into());
    ws.set_plan(p).unwrap();
    assert_eq!(ws.plans_for("2024-02").unwrap().len(), 1);
    assert!(ws.plans_for("2024-03").unwrap().is_empty());
    let mut p = ws.plans[0].clone();
    p.end_date = "2024-03-01".parse().unwrap();
    ws.set_plan(p).unwrap();
    assert_eq!(ws.plans_for("2024-03").unwrap().len(), 1);
    let stored = fs::read_to_string(ws.root.join("workspace.toml")).unwrap();
    assert!(!stored.contains("PRIVATE DOCUMENT CONTENT"));
    fs::remove_file(file).unwrap();
    assert!(Workspace::load(&ws.root).is_ok());
    let mut p = ws.plans[0].clone();
    p.person_sections.insert("outsider".into(), "Other".into());
    assert!(ws.set_plan(p).is_err());
    let mut p = ws.plans[0].clone();
    p.end_date = "2024-01-01".parse().unwrap();
    assert!(ws.set_plan(p).is_err());
}
#[test]
fn no_credentials_or_content_can_be_added_as_plan_fields() {
    let (_d, mut ws) = workspace();
    for reference in [
        "https://portal.test/rest/1/private-token/",
        "https://docs.google.com/document/d/x?access_token=secret",
        "https://user:password@example.com/doc",
        "ya29.fake",
        "sk-or-private",
        "line1\nline2",
    ] {
        let p = PlanReference::for_month("2026-09", reference.into(), None, None).unwrap();
        assert!(ws.set_plan(p).is_err());
    }
    let p = PlanReference::for_month(
        "2026-09",
        "https://docs.google.com/document/d/x".into(),
        None,
        None,
    )
    .unwrap();
    let mut value = serde_json::to_value(p).unwrap();
    value["google_oauth_credential"] = json!("private");
    assert!(serde_json::from_value::<PlanReference>(value).is_err());
    assert!(Workspace::load(&ws.root).unwrap().plans.is_empty());
}
#[test]
fn old_workspace_and_calculation_cli_file_and_stdin() {
    let (d, ws) = workspace();
    assert!(ws.scoring().unwrap().is_none());
    assert!(ws.plans.is_empty());
    assert_eq!(
        success(cli(&ws, &["scoring", "show"], None)),
        json!({"configured":false})
    );
    assert!(
        !cli(
            &ws,
            &["calculate", "--json"],
            Some(&serde_json::to_value(request()).unwrap())
        )
        .status
        .success()
    );
    install_scoring(&ws);
    assert_eq!(
        success(cli(&ws, &["scoring", "show"], None))["configured"],
        true
    );
    let input = serde_json::to_value(request()).unwrap();
    let out = success(cli(&ws, &["calculate", "--input-json", "-"], Some(&input)));
    assert_eq!(out["final_total"], 170000);
    assert_eq!(out["person"], "one");
    let file = d.path().join("request.json");
    fs::write(&file, input.to_string()).unwrap();
    assert_eq!(
        success(cli(
            &ws,
            &["calculate", "--input-json", file.to_str().unwrap()],
            None
        )),
        out
    );
    let text = fs::read_to_string(ws.root.join("workspace.toml")).unwrap();
    assert!(!text.contains("plans"));
    assert!(Workspace::load(&ws.root).is_ok());
}
#[tokio::test]
async fn mcp_calculator_is_pure_validates_and_respects_binding() {
    let (_d, ws) = workspace();
    install_scoring(&ws);
    let before = fs::read(ws.root.join("workspace.toml")).unwrap();
    let before_state = fs::read(ws.root.join("people/one/state.toml")).unwrap();
    let mut args = serde_json::to_value(request()).unwrap();
    args["workspace"] = json!(ws.root);
    let out = bkpi::mcp::call("kpi_calculate", args.clone(), Some(&ws.root))
        .await
        .unwrap();
    assert_eq!(out["final_total"], 170000);
    assert_eq!(
        bkpi::mcp::call("scoring_get", json!({"workspace":ws.root}), None)
            .await
            .unwrap()["configured"],
        true
    );
    let (_other, other) = workspace();
    assert!(
        bkpi::mcp::call("kpi_calculate", args.clone(), Some(&other.root))
            .await
            .is_err()
    );
    args["assessments"][0]["progress"] = json!(0.85);
    assert!(bkpi::mcp::call("kpi_calculate", args, None).await.is_err());
    assert_eq!(before, fs::read(ws.root.join("workspace.toml")).unwrap());
    assert_eq!(
        before_state,
        fs::read(ws.root.join("people/one/state.toml")).unwrap()
    );
}
#[test]
fn real_mcp_calculation_subprocess() {
    let (_d, ws) = workspace();
    install_scoring(&ws);
    let mut child = Command::new(env!("CARGO_BIN_EXE_bkpi"))
        .arg("--workspace")
        .arg(ws.root.parent().unwrap())
        .arg("mcp")
        .env("BKPI_CONFIG_DIR", ws.root.join("no-credentials"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for message in [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"scoring_get","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"kpi_calculate","arguments":request()}}),
    ] {
        writeln!(stdin, "{message}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let lines: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let tools = lines[1]["result"]["tools"].as_array().unwrap();
    let tool = tools.iter().find(|t| t["name"] == "kpi_calculate").unwrap();
    assert_eq!(tool["annotations"]["readOnlyHint"], true);
    assert_eq!(tool["annotations"]["openWorldHint"], false);
    assert_eq!(lines[2]["result"]["structuredContent"]["configured"], true);
    assert_eq!(
        lines[3]["result"]["structuredContent"]["final_total"],
        170000
    );
}
#[test]
fn agent_contract_is_present_in_shared_skill() {
    let skill = include_str!("../agent/skill/SKILL.md");

    for text in [
        "CURRENT conversation",
        "person-only",
        "current BKPI workspace",
        "connector is missing",
        "attach/export/paste",
        "person_sections",
        "unknown evidence ≠ partial payout",
        "kpi_calculate",
        "Use returned amounts verbatim",
        "latest explicitly approved revision",
    ] {
        assert!(skill.contains(text), "Missing instruction: {text}");
    }
}
#[cfg(feature = "standalone-ai")]
#[test]
fn standalone_ai_with_scoring_stops_before_network() {
    let (_d, ws) = workspace();
    install_scoring(&ws);
    fs::write(ws.root.join("people/one/kpi.md"), "# Rules\nDelivery").unwrap();
    let out = cli(&ws, &["ai", "analyze"], None);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("kpi_calculate"));
}

#[test]
fn google_credentials_are_redacted_even_in_cli_parse_errors() {
    let (_d, ws) = workspace();
    for token in [
        "ya29.private-test-token",
        "GOCSPX-private-test-secret",
        "1//private-refresh-token",
    ] {
        assert!(!bkpi::credentials::redact_text(token).contains("private"));
        let output = cli(&ws, &["plan", "set", "--not-an-option", token], None);
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stderr).contains(token));
    }
}

#[test]
fn arbitrary_threshold_order_zero_threshold_and_nonstandard_factors() {
    let mut c = config();
    c.criteria.truncate(1);
    c.max_payout = 40000;
    c.criteria[0].levels.push(bkpi::scoring::Level {
        id: "middle".into(),
        min_progress: Some(fixed("0.5")),
        factor: fixed("0.333333"),
    });
    c.criteria[0].levels.push(bkpi::scoring::Level {
        id: "low".into(),
        min_progress: Some(Fixed::ZERO),
        factor: fixed("0.1"),
    });
    let mut r = request();
    r.assessments.truncate(1);
    for (progress, level, amount) in [
        ("0", "low", 4000),
        ("0.499999", "low", 4000),
        ("0.5", "middle", 13333),
        ("0.85", "full", 40000),
    ] {
        r.assessments[0].progress = Some(fixed(progress));
        let out = calculate(&c, &r).unwrap();
        assert_eq!(out.criteria[0].resolved_level.as_deref(), Some(level));
        assert_eq!(out.final_total, Some(amount));
    }
}

#[test]
fn plan_cli_update_preserves_explicit_person_mapping_and_label() {
    let (_d, mut ws) = workspace();
    let mut plan = PlanReference::for_month(
        "2026-09",
        "https://docs.google.com/document/d/original".into(),
        None,
        Some("Agreed team plan".into()),
    )
    .unwrap();
    plan.person_sections
        .insert("one".into(), "Эдуард Шубин".into());
    ws.set_plan(plan).unwrap();
    success(cli(
        &ws,
        &[
            "plan",
            "set",
            "--period",
            "2026-09",
            "https://docs.google.com/document/d/revised",
        ],
        None,
    ));
    let loaded = Workspace::load(&ws.root).unwrap();
    assert_eq!(loaded.plans[0].person_sections["one"], "Эдуард Шубин");
    assert_eq!(loaded.plans[0].label, "Agreed team plan");
}

#[test]
fn removing_person_keeps_shared_plan_and_other_mappings() {
    let (_d, mut ws) = workspace();
    let mut plan = PlanReference::for_month(
        "2026-09",
        "https://docs.google.com/document/d/team".into(),
        None,
        None,
    )
    .unwrap();
    plan.person_sections
        .insert("one".into(), "Эдуард Шубин".into());
    plan.person_sections
        .insert("two".into(), "Иван Иванов".into());
    ws.set_plan(plan).unwrap();
    assert!(
        cli(&ws, &["person", "remove", "self"], None)
            .status
            .success()
    );
    let loaded = Workspace::load(&ws.root).unwrap();
    assert_eq!(loaded.people.len(), 1);
    assert_eq!(loaded.plans.len(), 1);
    assert_eq!(loaded.plans[0].person_sections.len(), 1);
    assert!(loaded.plans[0].person_sections.contains_key("two"));
    assert!(ws.root.join("people/one/state.toml").exists());
}
