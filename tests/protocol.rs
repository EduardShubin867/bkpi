use bkpi::workspace::{Person, Workspace};
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};
#[test]
fn actual_stdio_protocol_and_json_cli() {
    let dir = tempfile::TempDir::new().unwrap();
    let config = tempfile::TempDir::new().unwrap();
    let mut ws = Workspace::create(dir.path()).unwrap();
    ws.add(Person {
        id: "one".into(),
        name: "One".into(),
        integration: "portal".into(),
        bitrix_user_id: "1".into(),
        is_self: true,
        kpi: "people/one/kpi.md".into(),
    })
    .unwrap();
    let mut process = Command::new(env!("CARGO_BIN_EXE_bkpi"))
        .arg("mcp")
        .current_dir(dir.path())
        .env("BKPI_CONFIG_DIR", config.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"bkpi_kpi_document_get","arguments":{}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"bkpi_mapping_set","arguments":{"task_id":"10","criterion":"Custom","period":"2026-09"}}}),
    ];
    {
        let mut stdin = process.stdin.take().unwrap();
        for request in requests {
            writeln!(stdin, "{request}").unwrap();
        }
    }
    let output = process.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let lines: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|v| serde_json::from_str(v).unwrap())
        .collect();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0]["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(lines[1]["result"]["tools"].as_array().unwrap().len(), 16);
    assert_eq!(
        lines[2]["result"]["structuredContent"]["kpi_document"]["status"],
        "missing"
    );
    assert_eq!(lines[3]["result"]["isError"], false);
    let output = Command::new(env!("CARGO_BIN_EXE_bkpi"))
        .args(["agent", "call", "bkpi_local_state_get"])
        .current_dir(dir.path())
        .env("BKPI_CONFIG_DIR", config.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["state"]["mappings"][0]["criterion"], "Custom");
}
#[test]
fn secret_argv_not_supported() {
    let output = Command::new(env!("CARGO_BIN_EXE_bkpi"))
        .args(["integration", "add", "--webhook", "fake"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}
#[test]
fn no_workspace_errors_have_empty_stdout() {
    let dir = tempfile::TempDir::new().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_bkpi"))
        .args(["agent", "snapshot", "--json"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn legacy_secret_argument_is_redacted_in_parse_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_bkpi"))
        .args(["init", "https://portal.test/rest/1/never-echo/"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        !String::from_utf8(output.stderr)
            .unwrap()
            .contains("never-echo")
    );
}

#[cfg(feature = "standalone-ai")]
#[test]
fn standalone_ai_without_document_stops_before_credentials_or_network() {
    let dir = tempfile::TempDir::new().unwrap();
    let config = tempfile::TempDir::new().unwrap();
    let mut ws = Workspace::create(dir.path()).unwrap();
    ws.add(Person {
        id: "one".into(),
        name: "One".into(),
        integration: "missing".into(),
        bitrix_user_id: "1".into(),
        is_self: true,
        kpi: "people/one/kpi.md".into(),
    })
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_bkpi"))
        .args(["ai", "analyze"])
        .current_dir(dir.path())
        .env("BKPI_CONFIG_DIR", config.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("No KPI document")
    );
}
