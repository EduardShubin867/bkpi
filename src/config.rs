use crate::credentials::{
    CredentialRef, CredentialStore, Secret, SystemStore, private_write, webhook,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Integration {
    pub id: String,
    pub display_name: String,
    pub base_url: String,
    pub credential: CredentialRef,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub integrations: Vec<Integration>,
    pub openrouter: Option<AiConfig>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiConfig {
    pub credential: CredentialRef,
    pub model: String,
    pub zdr: bool,
    pub deny_data_collection: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            version: 2,
            integrations: vec![],
            openrouter: None,
        }
    }
}
impl Config {
    pub fn root() -> Result<PathBuf> {
        if let Some(root) = std::env::var_os("BKPI_CONFIG_DIR") {
            return Ok(PathBuf::from(root));
        }
        Ok(dirs::config_dir()
            .context("No config directory")?
            .join("bkpi"))
    }
    pub fn store() -> Result<SystemStore> {
        Ok(SystemStore {
            root: Self::root()?,
        })
    }
    pub fn load() -> Result<Self> {
        Self::load_at(&Self::root()?)
    }
    pub fn load_at(root: &Path) -> Result<Self> {
        let path = root.join("config.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path)?;
        let value: toml::Value = toml::from_str(&text)
            .map_err(|_| anyhow::anyhow!("Invalid global config; contents suppressed"))?;
        ensure!(
            value.get("version").is_some(),
            "Legacy config detected. Run bkpi config migrate (creates a private backup)"
        );
        let config: Self = value
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid global config schema"))?;
        config.validate()?;
        Ok(config)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            matches!(self.version, 1 | 2),
            "Unsupported global config version"
        );
        if let Some(ai) = &self.openrouter {
            ai.credential.validate()?;
        }
        let mut ids = std::collections::BTreeSet::new();
        for i in &self.integrations {
            crate::workspace::validate_id(&i.id)?;
            i.credential.validate()?;
            ensure!(ids.insert(&i.id), "Duplicate integration ID");
            let url = url::Url::parse(&i.base_url)
                .map_err(|_| anyhow::anyhow!("Invalid integration origin"))?;
            ensure!(
                url.origin().ascii_serialization() == i.base_url && url.scheme() == "https",
                "Integration metadata must contain only HTTPS origin"
            );
        }
        Ok(())
    }
    pub fn save(&self) -> Result<()> {
        self.save_at(&Self::root()?)
    }
    pub fn save_at(&self, root: &Path) -> Result<()> {
        self.validate()?;
        let path = root.join("config.toml");
        if path.exists() {
            let original = fs::read_to_string(&path)?;
            let value: toml::Value = toml::from_str(&original)
                .map_err(|_| anyhow::anyhow!("Invalid existing config; contents suppressed"))?;
            if value.get("version").and_then(toml::Value::as_integer) == Some(1) {
                let backup = root.join("config.v1-backup.toml");
                if backup.exists() {
                    ensure!(
                        !backup.is_symlink() && fs::read_to_string(&backup)? == original,
                        "Schema backup differs; retain it and choose a new backup location before retrying"
                    );
                } else {
                    private_write(&backup, original.as_bytes())?;
                }
            }
        }
        let mut next = self.clone();
        next.version = 2;
        private_write(&path, toml::to_string_pretty(&next)?.as_bytes())
    }
    pub fn integration(&self, id: &str) -> Result<&Integration> {
        self.integrations
            .iter()
            .find(|i| i.id == id)
            .context("Integration not configured")
    }
    pub fn client(&self, id: &str) -> Result<crate::bitrix::BitrixClient> {
        let integration = self.integration(id)?;
        let secret = Self::store()?.load(&integration.credential)?;
        if integration.credential.backend() == crate::credentials::Backend::Env {
            let (_, origin) = webhook(secret.expose())?;
            ensure!(
                origin == integration.base_url,
                "Environment webhook belongs to a different integration origin"
            );
        }
        crate::bitrix::BitrixClient::new(secret.expose())
    }
}
pub fn migrate(root: &Path, store: &dyn CredentialStore, fallback: bool) -> Result<bool> {
    let path = root.join("config.toml");
    if !path.exists() {
        return Ok(false);
    }
    let original = fs::read_to_string(&path)?;
    let old: toml::Value = toml::from_str(&original)
        .map_err(|_| anyhow::anyhow!("Invalid legacy config; contents suppressed"))?;
    if old.get("version").is_some() {
        let config = Config::load_at(root)?;
        if config.version == 1 {
            config.save_at(root)?;
            return Ok(true);
        }
        return Ok(false);
    }
    let raw = old
        .get("bitrix")
        .and_then(|v| v.get("webhook_url"))
        .and_then(toml::Value::as_str)
        .context("No legacy webhook found")?;
    let (secret, base_url) = webhook(raw)?;
    // Backup precedes every credential/config mutation and remains recoverable on failure.
    let backup = root.join("config.legacy-backup.toml");
    if backup.exists() {
        ensure!(
            fs::read_to_string(&backup)? == original,
            "Legacy backup already exists with different contents"
        );
    } else {
        private_write(&backup, original.as_bytes())?;
    }
    let credential = store.save("bitrix-default", &secret, fallback)?;
    let mut config = Config {
        integrations: vec![Integration {
            id: "default".into(),
            display_name: "Migrated Bitrix".into(),
            base_url,
            credential,
        }],
        ..Default::default()
    };
    if let Some(ai) = old.get("openrouter")
        && let Some(key) = ai
            .get("api_key")
            .and_then(toml::Value::as_str)
            .filter(|v| !v.trim().is_empty())
    {
        config.openrouter = Some(AiConfig {
            credential: store.save("openrouter", &Secret::new(key.into()), fallback)?,
            model: ai
                .get("model")
                .and_then(toml::Value::as_str)
                .unwrap_or("openrouter/auto")
                .into(),
            zdr: ai.get("zdr").and_then(toml::Value::as_bool).unwrap_or(true),
            deny_data_collection: ai
                .get("deny_data_collection")
                .and_then(toml::Value::as_bool)
                .unwrap_or(true),
        });
    }
    config.save_at(root)?;
    eprintln!(
        "Migrated credentials. Legacy KPI plans/evidence remain in config.legacy-backup.toml (contains secrets, keep private). No legacy rubric was applied. Create a workspace and provide its KPI document."
    );
    Ok(true)
}

/// Derive metadata from the hostname only, never from a webhook path/token.
pub fn integration_id(origin: &str, existing: &[Integration]) -> Result<String> {
    let url = url::Url::parse(origin).map_err(|_| anyhow::anyhow!("Invalid integration origin"))?;
    let host = url
        .host_str()
        .context("Integration hostname missing")?
        .trim_end_matches('.');
    let host = host.strip_prefix("www.").unwrap_or(host);
    let base = [
        ".bitrix24.ru",
        ".bitrix24.com",
        ".bitrix24.eu",
        ".bitrix24.de",
        ".bitrix24.net",
    ]
    .iter()
    .find_map(|suffix| host.strip_suffix(suffix))
    .unwrap_or(host);
    let base = crate::workspace::slug(base);
    let base = if base.is_empty() {
        "bitrix".to_string()
    } else {
        base
    };
    let mut id = base.clone();
    let mut n = 2;
    while existing.iter().any(|i| i.id == id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    Ok(id)
}

/// Destination is verified before the configuration commit. A failed commit retains the source.
/// Fresh account names prevent failed moves from overwriting credentials used elsewhere.
pub fn move_credential(
    root: &Path,
    store: &dyn CredentialStore,
    id: &str,
    backend: crate::credentials::Backend,
    env_var: Option<&str>,
    confirm_env: bool,
) -> Result<bool> {
    use crate::credentials::{Backend, CredentialRef};
    let mut config = Config::load_at(root)?;
    let integration = config.integration(id)?.clone();
    let old = integration.credential.clone();
    if old.backend() == backend && backend != Backend::Env {
        return Ok(false);
    }
    let next = if backend == Backend::Env {
        ensure!(
            confirm_env,
            "Set the variable in the BKPI process environment, then retry with --confirm-env; configuration unchanged"
        );
        let name =
            env_var.context("Use --env-var NAME; BKPI never writes environment variables")?;
        let reference = CredentialRef::Env {
            account: name.into(),
        };
        reference.validate()?;
        let secret = store.load(&reference)?;
        let (_, origin) = webhook(secret.expose())?;
        ensure!(
            origin == integration.base_url,
            "Environment webhook belongs to a different integration origin"
        );
        reference
    } else {
        ensure!(env_var.is_none(), "--env-var applies only to env storage");
        let secret = store.load(&old)?;
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let next = store.set(&format!("bitrix-{id}-{suffix}"), &secret, backend)?;
        ensure!(
            next.backend() == backend,
            "Credential store returned an unexpected backend"
        );
        let verified = store.load(&next)?;
        ensure!(
            verified.expose() == secret.expose(),
            "Destination credential verification failed; configuration unchanged"
        );
        next
    };
    if next == old {
        return Ok(false);
    }
    config
        .integrations
        .iter_mut()
        .find(|i| i.id == id)
        .context("Integration missing")?
        .credential = next;
    config.save_at(root)?;
    let shared = config.integrations.iter().any(|i| i.credential == old)
        || config
            .openrouter
            .as_ref()
            .is_some_and(|a| a.credential == old);
    // Env moves retain the source as recovery: future shells/agent hosts may lack the variable.
    if !shared && backend != Backend::Env && store.remove(&old).is_err() {
        return Ok(true);
    }
    Ok(false)
}
