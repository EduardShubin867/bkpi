use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::Path};

/// Intentionally neither Debug nor Serialize.
pub struct Secret(String);
impl Secret {
    pub fn new(value: String) -> Self {
        Self(value)
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
    pub fn redact(&self, value: &str) -> String {
        let mut safe = if self.0.is_empty() {
            value.into()
        } else {
            value.replace(&self.0, "[REDACTED]")
        };
        if let Ok(url) = url::Url::parse(&self.0) {
            for part in url.path_segments().into_iter().flatten().skip(2) {
                if !part.is_empty() {
                    safe = safe.replace(part, "[REDACTED]");
                }
            }
        }
        redact_text(&safe)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Backend {
    File,
    Keychain,
    Env,
}
impl Backend {
    pub fn name(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Keychain => "keychain",
            Self::Env => "env",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "store", rename_all = "snake_case", deny_unknown_fields)]
pub enum CredentialRef {
    #[serde(alias = "keychain")]
    Keyring {
        account: String,
    },
    File {
        account: String,
    },
    Env {
        account: String,
    },
}
impl CredentialRef {
    pub fn backend(&self) -> Backend {
        match self {
            Self::Keyring { .. } => Backend::Keychain,
            Self::File { .. } => Backend::File,
            Self::Env { .. } => Backend::Env,
        }
    }
    pub fn account(&self) -> &str {
        match self {
            Self::Keyring { account } | Self::File { account } | Self::Env { account } => account,
        }
    }
    pub fn validate(&self) -> Result<()> {
        if self.backend() == Backend::Env {
            validate_env(self.account())
        } else {
            crate::workspace::validate_id(self.account())
        }
    }
}
pub fn validate_env(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.bytes().enumerate().all(|(i, c)| c == b'_'
                || c.is_ascii_alphabetic()
                || (i > 0 && c.is_ascii_digit())),
        "Invalid environment variable name"
    );
    Ok(())
}

fn private_directory(path: &Path) -> Result<()> {
    if !path.exists() {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path)?;
    }
    ensure!(
        !path.is_symlink() && path.is_dir(),
        "Refusing symlink or non-directory credential parent"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            fs::metadata(path)?.permissions().mode() & 0o022 == 0,
            "Credential directory permissions are too broad; run bkpi config fix-permissions"
        );
    }
    Ok(())
}
pub fn check_file_permissions(path: &Path) -> Result<()> {
    ensure!(!path.is_symlink(), "Refusing symlink credential file");
    let metadata = fs::metadata(path).context("Credential file unavailable")?;
    ensure!(metadata.is_file(), "Credential must be a regular file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            metadata.permissions().mode() & 0o777 == 0o600,
            "Credentials file permissions are too broad or invalid; run bkpi config fix-permissions (0600 required)"
        );
    }
    Ok(())
}
pub fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("File has no parent")?;
    private_directory(parent)?;
    ensure!(!path.is_symlink(), "Refusing symlink destination");
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path)
        .map_err(|_| anyhow::anyhow!("Cannot replace local file"))?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
pub trait CredentialStore {
    // Kept for source compatibility: true explicitly selects file, false keychain.
    fn save(&self, account: &str, secret: &Secret, file_fallback: bool) -> Result<CredentialRef>;
    fn load(&self, reference: &CredentialRef) -> Result<Secret>;
    fn remove(&self, reference: &CredentialRef) -> Result<()>;
    fn set(&self, account: &str, secret: &Secret, backend: Backend) -> Result<CredentialRef> {
        ensure!(
            backend != Backend::Env,
            "Environment credentials are read-only; set the variable outside BKPI"
        );
        self.save(account, secret, backend == Backend::File)
    }
    fn exists(&self, reference: &CredentialRef) -> bool {
        self.load(reference).is_ok()
    }
}
pub struct SystemStore {
    pub root: std::path::PathBuf,
}
impl SystemStore {
    pub fn file(&self, account: &str) -> Result<std::path::PathBuf> {
        crate::workspace::validate_id(account)?;
        Ok(self.root.join("secrets").join(format!("{account}.secret")))
    }
    pub fn check_permissions(&self, reference: &CredentialRef) -> Result<()> {
        if let CredentialRef::File { account } = reference {
            // Reading never creates directories.
            for path in [&self.root, &self.root.join("secrets")] {
                ensure!(!path.is_symlink(), "Refusing symlink credential directory");
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    ensure!(
                        fs::metadata(path)?.permissions().mode() & 0o022 == 0,
                        "Credential directory permissions are too broad; run bkpi config fix-permissions"
                    );
                }
            }
            check_file_permissions(&self.file(account)?)?;
        }
        Ok(())
    }
    pub fn fix_permissions(&self) -> Result<()> {
        #[cfg(not(unix))]
        bail!(
            "Unix permission repair is unavailable on this platform; inspect user directory ACLs"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Validate all entries before changing anything. Never follow links.
            let mut paths = vec![(self.root.clone(), 0o700)];
            let dir = self.root.join("secrets");
            if dir.exists() || dir.is_symlink() {
                ensure!(!dir.is_symlink(), "Refusing symlink credential directory");
                paths.push((dir.clone(), 0o700));
                for entry in fs::read_dir(&dir)? {
                    let entry = entry?;
                    ensure!(
                        entry.file_type()?.is_file(),
                        "Refusing non-regular credential entry"
                    );
                    paths.push((entry.path(), 0o600));
                }
            }
            for (path, _) in &paths {
                ensure!(!path.is_symlink(), "Refusing symlink permission repair");
            }
            for (path, mode) in paths {
                fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
            }
            Ok(())
        }
    }
}
impl CredentialStore for SystemStore {
    fn save(&self, account: &str, secret: &Secret, file_fallback: bool) -> Result<CredentialRef> {
        crate::workspace::validate_id(account)?;
        ensure!(!secret.expose().trim().is_empty(), "Credential is empty");
        if !file_fallback {
            let entry = keyring::Entry::new("bkpi", account)
                .map_err(|_| anyhow::anyhow!("OS credential store unavailable"))?;
            entry
                .set_password(secret.expose())
                .map_err(|_| anyhow::anyhow!("Cannot save in OS credential store"))?;
            return Ok(CredentialRef::Keyring {
                account: account.into(),
            });
        }
        private_directory(&self.root)?;
        let reference = CredentialRef::File {
            account: account.into(),
        };
        let path = self.file(account)?;
        if path.exists() || path.is_symlink() {
            self.check_permissions(&reference)?;
        }
        private_write(&path, secret.expose().as_bytes())?;
        Ok(reference)
    }
    fn load(&self, reference: &CredentialRef) -> Result<Secret> {
        reference.validate()?;
        let value = match reference {
            CredentialRef::Keyring { account } => keyring::Entry::new("bkpi", account)
                .and_then(|e| e.get_password()).map_err(|_| anyhow::anyhow!("OS credential unavailable; update the integration credential"))?,
            CredentialRef::File { account } => {
                self.check_permissions(reference)?;
                fs::read_to_string(self.file(account)?).map_err(|_| anyhow::anyhow!("Credential file unavailable"))?
            }
            CredentialRef::Env { account } => std::env::var(account).map_err(|_| anyhow::anyhow!("Credential environment variable is missing or not Unicode; set it in the BKPI process environment"))?,
        };
        ensure!(!value.trim().is_empty(), "Credential is empty");
        Ok(Secret(value))
    }
    fn remove(&self, reference: &CredentialRef) -> Result<()> {
        reference.validate()?;
        match reference {
            CredentialRef::Keyring { account } => {
                match keyring::Entry::new("bkpi", account).and_then(|e| e.delete_credential()) {
                    Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                    Err(_) => bail!("Cannot remove OS credential"),
                }
            }
            CredentialRef::File { account } => {
                let path = self.file(account)?;
                ensure!(
                    !path.is_symlink()
                        && !self.root.is_symlink()
                        && !self.root.join("secrets").is_symlink(),
                    "Refusing symlink credential removal"
                );
                match fs::remove_file(path) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(_) => bail!("Cannot remove credential file"),
                }
            }
            CredentialRef::Env { .. } => Ok(()), // BKPI never changes the process environment.
        }
    }
}
pub fn webhook(raw: &str) -> Result<(Secret, String)> {
    let url = url::Url::parse(raw.trim()).map_err(|_| anyhow::anyhow!("Invalid webhook URL"))?;
    ensure!(
        url.scheme() == "https" && url.host_str().is_some(),
        "Webhook must use HTTPS"
    );
    ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "Webhook must not contain URL userinfo, query or fragment"
    );
    let parts: Vec<_> = url.path().trim_matches('/').split('/').collect();
    ensure!(
        parts.len() == 3
            && parts[0] == "rest"
            && parts[1].chars().all(|c| c.is_ascii_digit())
            && !parts[1].is_empty()
            && !parts[2].is_empty(),
        "Expected Bitrix incoming webhook path /rest/user/token/"
    );
    let base = url.origin().ascii_serialization();
    Ok((Secret(url.to_string()), base))
}

/// Defense in depth for accidentally pasted credentials in documents/remote text.
/// This does not require loading any credential store.
pub fn redact_text(text: &str) -> String {
    static WEBHOOK: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r#"(?i)https?://[^\s"<>]+/rest/[0-9]+/[^/\s"<>]+/?"#)
            .expect("static webhook regex")
    });
    static KEY: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?:sk-or-|ya29\.|GOCSPX-|1//)[A-Za-z0-9_.-]+")
            .expect("static key regex")
    });
    KEY.replace_all(
        &WEBHOOK.replace_all(text, "[REDACTED WEBHOOK]"),
        "[REDACTED KEY]",
    )
    .into_owned()
}
pub fn redact_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => *s = redact_text(s),
        serde_json::Value::Array(values) => values.iter_mut().for_each(redact_value),
        serde_json::Value::Object(values) => {
            let old = std::mem::take(values);
            for (key, mut value) in old {
                redact_value(&mut value);
                values.insert(redact_text(&key), value);
            }
        }
        _ => {}
    }
}
