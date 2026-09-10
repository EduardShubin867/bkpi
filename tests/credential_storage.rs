use bkpi::{
    config::{Config, Integration, migrate, move_credential},
    credentials::{
        Backend, CredentialRef, CredentialStore, Secret, SystemStore, private_write, redact_text,
    },
};
use std::{collections::BTreeMap, fs, process::Command, sync::Mutex};
use tempfile::TempDir;
const WEBHOOK: &str = "https://portal.test/rest/1/fixture-token/";
fn config(reference: CredentialRef) -> Config {
    Config {
        integrations: vec![Integration {
            id: "portal".into(),
            display_name: "Астро-Волга".into(),
            base_url: "https://portal.test".into(),
            credential: reference,
        }],
        ..Config::default()
    }
}
fn file() -> CredentialRef {
    CredentialRef::File {
        account: "bitrix-portal".into(),
    }
}
#[test]
fn file_crud_atomic_replacement_and_config_reference() {
    let dir = TempDir::new().unwrap();

    let store = SystemStore {
        root: dir.path().into(),
    };

    let reference = store
        .set("bitrix-portal", &Secret::new(WEBHOOK.into()), Backend::File)
        .unwrap();

    assert!(store.exists(&reference));

    config(reference.clone()).save_at(dir.path()).unwrap();

    assert!(
        !fs::read_to_string(dir.path().join("config.toml"))
            .unwrap()
            .contains("fixture-token")
    );

    let path = store.file(reference.account()).unwrap();

    // Unix allows us to keep the old inode open while atomically replacing
    // the path. Windows does not guarantee the same replace-over-open-handle
    // semantics, so this part of the atomicity test is Unix-only.
    #[cfg(unix)]
    let mut old = fs::File::open(&path).unwrap();

    // On Windows verify the original credential before replacement without
    // retaining an open handle to the destination.
    #[cfg(windows)]
    assert_eq!(fs::read_to_string(&path).unwrap(), WEBHOOK);

    store
        .set(
            reference.account(),
            &Secret::new("replacement".into()),
            Backend::File,
        )
        .unwrap();

    #[cfg(unix)]
    {
        use std::io::Read;

        let mut previous = String::new();
        old.read_to_string(&mut previous).unwrap();

        // The already-open Unix file still refers to the complete old inode.
        assert_eq!(previous, WEBHOOK);
    }

    assert_eq!(store.load(&reference).unwrap().expose(), "replacement");

    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);

    store.remove(&reference).unwrap();
    store.remove(&reference).unwrap();

    assert!(!store.exists(&reference));
}
#[cfg(unix)]
#[test]
fn insecure_permissions_and_symlinks_fail_closed_and_repair_is_explicit() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = TempDir::new().unwrap();
    let store = SystemStore {
        root: dir.path().join("config"),
    };
    let reference = store
        .set("bitrix-portal", &Secret::new(WEBHOOK.into()), Backend::File)
        .unwrap();
    let path = store.file(reference.account()).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let error = store.load(&reference).err().unwrap().to_string();
    assert!(error.contains("fix-permissions"));
    assert!(!error.contains("fixture-token"));
    assert!(
        store
            .set(
                reference.account(),
                &Secret::new("new".into()),
                Backend::File
            )
            .is_err()
    );
    store.fix_permissions().unwrap();
    assert!(store.exists(&reference));
    let target = dir.path().join("target");
    fs::write(&target, "untouched").unwrap();
    fs::remove_file(&path).unwrap();
    symlink(&target, &path).unwrap();
    assert!(store.load(&reference).is_err());
    assert!(store.fix_permissions().is_err());
    assert!(
        store
            .set(
                reference.account(),
                &Secret::new("new".into()),
                Backend::File
            )
            .is_err()
    );
    assert_eq!(fs::read_to_string(target).unwrap(), "untouched");
}
#[test]
fn schema_upgrade_backs_up_and_preserves_keyring_references_without_accessing_store() {
    let dir = TempDir::new().unwrap();
    let mut c = config(CredentialRef::Keyring {
        account: "existing".into(),
    });
    c.version = 1;
    let original = toml::to_string_pretty(&c).unwrap();
    private_write(&dir.path().join("config.toml"), original.as_bytes()).unwrap();
    let before = Config::load_at(dir.path()).unwrap();
    assert_eq!(before.version, 1);
    before.save_at(dir.path()).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("config.v1-backup.toml")).unwrap(),
        original
    );
    let after = Config::load_at(dir.path()).unwrap();
    assert_eq!(after.version, 2);
    assert_eq!(
        after.integrations[0].credential,
        c.integrations[0].credential
    );
    after.save_at(dir.path()).unwrap();
}
#[test]
fn legacy_cli_migration_defaults_to_file_and_retains_backup() {
    let dir = TempDir::new().unwrap();
    let old = format!("[bitrix]\nwebhook_url='{WEBHOOK}'\n[openrouter]\napi_key='sk-or-fixture'\n");
    private_write(&dir.path().join("config.toml"), old.as_bytes()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_bkpi"))
        .args(["config", "migrate"])
        .env("BKPI_CONFIG_DIR", dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("fixture-token"));
    let c = Config::load_at(dir.path()).unwrap();
    assert_eq!(c.integrations[0].credential.backend(), Backend::File);
    assert_eq!(c.openrouter.unwrap().credential.backend(), Backend::File);
    assert_eq!(
        fs::read_to_string(dir.path().join("config.legacy-backup.toml")).unwrap(),
        old
    );
    assert!(
        !migrate(
            dir.path(),
            &SystemStore {
                root: dir.path().into()
            },
            true
        )
        .unwrap()
    );
}
#[derive(Default)]
struct MockStore {
    values: Mutex<BTreeMap<String, String>>,
    events: Mutex<Vec<String>>,
    fail: &'static str,
}
impl MockStore {
    fn key(r: &CredentialRef) -> String {
        format!("{}:{}", r.backend().name(), r.account())
    }
    fn seed(&self, r: &CredentialRef) {
        self.values
            .lock()
            .unwrap()
            .insert(Self::key(r), WEBHOOK.into());
    }
}
impl CredentialStore for MockStore {
    fn save(&self, a: &str, s: &Secret, f: bool) -> anyhow::Result<CredentialRef> {
        self.events.lock().unwrap().push("save".into());
        anyhow::ensure!(self.fail != "save", "save failed");
        let r = if f {
            CredentialRef::File { account: a.into() }
        } else {
            CredentialRef::Keyring { account: a.into() }
        };
        self.values.lock().unwrap().insert(
            Self::key(&r),
            if self.fail == "verify" {
                "wrong".into()
            } else {
                s.expose().into()
            },
        );
        Ok(r)
    }
    fn load(&self, r: &CredentialRef) -> anyhow::Result<Secret> {
        self.events.lock().unwrap().push("load".into());
        self.values
            .lock()
            .unwrap()
            .get(&Self::key(r))
            .cloned()
            .map(Secret::new)
            .ok_or_else(|| anyhow::anyhow!("missing"))
    }
    fn remove(&self, r: &CredentialRef) -> anyhow::Result<()> {
        self.events.lock().unwrap().push("delete".into());
        anyhow::ensure!(self.fail != "delete", "delete failed");
        self.values.lock().unwrap().remove(&Self::key(r));
        Ok(())
    }
}
#[test]
fn moves_verify_commit_then_delete_in_both_directions_and_warn_on_delete_failure() {
    for (backend, old) in [
        (Backend::Keychain, file()),
        (
            Backend::File,
            CredentialRef::Keyring {
                account: "existing".into(),
            },
        ),
    ] {
        for fail in ["", "delete"] {
            let dir = TempDir::new().unwrap();
            config(old.clone()).save_at(dir.path()).unwrap();
            let store = MockStore {
                fail,
                ..Default::default()
            };
            store.seed(&old);
            assert_eq!(
                move_credential(dir.path(), &store, "portal", backend, None, false).unwrap(),
                fail == "delete"
            );
            let next = Config::load_at(dir.path()).unwrap().integrations[0]
                .credential
                .clone();
            assert_eq!(next.backend(), backend);
            assert_eq!(store.load(&next).unwrap().expose(), WEBHOOK);
            assert_eq!(
                &store.events.lock().unwrap()[..4],
                ["load", "save", "load", "delete"]
            );
            assert_eq!(store.exists(&old), fail == "delete");
        }
    }
}
#[test]
fn move_failure_never_deletes_source_or_changes_config() {
    for fail in ["save", "verify", "config"] {
        let dir = TempDir::new().unwrap();
        let mut c = config(file());
        c.version = 1;
        private_write(
            &dir.path().join("config.toml"),
            toml::to_string(&c).unwrap().as_bytes(),
        )
        .unwrap();
        if fail == "config" {
            private_write(
                &dir.path().join("config.v1-backup.toml"),
                b"existing different backup",
            )
            .unwrap();
        }
        let store = MockStore {
            fail,
            ..Default::default()
        };
        store.seed(&file());
        assert!(
            move_credential(dir.path(), &store, "portal", Backend::Keychain, None, false).is_err()
        );
        assert_eq!(
            Config::load_at(dir.path()).unwrap().integrations[0].credential,
            file()
        );
        assert!(store.exists(&file()));
        assert!(!store.events.lock().unwrap().contains(&"delete".into()));
    }
}
#[test]
fn move_retains_shared_source_and_env_requires_confirmation_and_present_value() {
    let dir = TempDir::new().unwrap();
    let mut c = config(file());
    let mut other = c.integrations[0].clone();
    other.id = "other".into();
    c.integrations.push(other);
    c.save_at(dir.path()).unwrap();
    let store = MockStore::default();
    store.seed(&file());
    move_credential(dir.path(), &store, "portal", Backend::Keychain, None, false).unwrap();
    assert!(store.exists(&file()));
    let env = CredentialRef::Env {
        account: "BKPI_TEST_CREDENTIAL".into(),
    };
    assert!(
        move_credential(
            dir.path(),
            &store,
            "other",
            Backend::Env,
            Some(env.account()),
            true
        )
        .is_err()
    );
    store.seed(&env);
    assert!(
        move_credential(
            dir.path(),
            &store,
            "other",
            Backend::Env,
            Some(env.account()),
            false
        )
        .is_err()
    );
    move_credential(
        dir.path(),
        &store,
        "other",
        Backend::Env,
        Some(env.account()),
        true,
    )
    .unwrap();
    assert_eq!(
        Config::load_at(dir.path())
            .unwrap()
            .integration("other")
            .unwrap()
            .credential,
        env
    );
    assert!(store.exists(&file()));
}
#[test]
fn env_resolution_missing_and_metadata_only_cli() {
    let dir = TempDir::new().unwrap();
    config(CredentialRef::Env {
        account: "BKPI_TEST_CREDENTIAL".into(),
    })
    .save_at(dir.path())
    .unwrap();
    for value in [None, Some(""), Some(WEBHOOK)] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_bkpi"));
        cmd.args(["integration", "credential", "show", "portal", "--json"])
            .env("BKPI_CONFIG_DIR", dir.path())
            .env_remove("BKPI_TEST_CREDENTIAL");
        if let Some(value) = value {
            cmd.env("BKPI_TEST_CREDENTIAL", value);
        }
        let out = cmd.output().unwrap();
        assert!(out.status.success());
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(!text.contains("fixture-token"));
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["configured"], value == Some(WEBHOOK));
        assert_eq!(v["backend"], "env");
    }
}
#[test]
fn redaction_handles_webhook_keys_arbitrary_values_and_empty_secrets() {
    let text = format!("anyhow: request to {WEBHOOK}tasks.task.list failed: sk-or-fixture");
    let safe = redact_text(&text);
    assert!(!safe.contains("fixture-token"));
    assert!(!safe.contains("sk-or-fixture"));
    assert_eq!(
        Secret::new("arbitrary-sensitive-value".into()).redact("error: arbitrary-sensitive-value"),
        "error: [REDACTED]"
    );
    assert_eq!(Secret::new("".into()).redact("normal"), "normal");
}

#[test]
fn env_setup_is_pending_without_requesting_or_storing_a_secret() {
    let dir = TempDir::new().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_bkpi"))
        .args([
            "integration",
            "add",
            "--backend",
            "env",
            "--base-url",
            "https://portal.test",
            "--env-var",
            "BKPI_TEST_UNSET",
            "--name",
            "Portal",
        ])
        .env("BKPI_CONFIG_DIR", dir.path())
        .env_remove("BKPI_TEST_UNSET")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("NOT usable"));
    assert!(!dir.path().join("secrets").exists());
    let c = Config::load_at(dir.path()).unwrap();
    assert_eq!(c.integrations[0].credential.backend(), Backend::Env);
    let out = Command::new(env!("CARGO_BIN_EXE_bkpi"))
        .args(["doctor"])
        .env("BKPI_CONFIG_DIR", dir.path())
        .env(
            "BKPI_TEST_UNSET",
            "https://another.test/rest/1/fixture-token/",
        )
        .output()
        .unwrap();
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("different integration origin"));
    assert!(!text.contains("fixture-token"));
}
#[cfg(unix)]
#[test]
fn doctor_reports_insecure_permissions_and_cli_fix_repairs_them() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    let store = SystemStore {
        root: dir.path().into(),
    };
    store
        .set(
            file().account(),
            &Secret::new(WEBHOOK.into()),
            Backend::File,
        )
        .unwrap();
    config(file()).save_at(dir.path()).unwrap();
    fs::set_permissions(
        store.file(file().account()).unwrap(),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_bkpi"))
        .args(["doctor"])
        .env("BKPI_CONFIG_DIR", dir.path())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("fix-permissions"));
    assert!(!text.contains("fixture-token"));
    let out = Command::new(env!("CARGO_BIN_EXE_bkpi"))
        .args(["config", "fix-permissions"])
        .env("BKPI_CONFIG_DIR", dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(store.exists(&file()));
}
