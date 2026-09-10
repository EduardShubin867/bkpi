use crate::credentials::private_write;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

pub const KPI_TEMPLATE: &str = "# KPI\n\nKPI пока не добавлены.\n\nДобавьте сюда постоянные KPI для этого workspace\nили передайте KPI агенту прямо в текущем чате для разового анализа.\n";
const LEGACY_KPI_TEMPLATE: &str = "# KPI\n\nВставьте сюда ваши KPI на текущий период.\n";

/// Normalize pasted Finder/Explorer paths without interpreting shell commands.
pub fn input_path(raw: &str, base: &Path) -> Result<PathBuf> {
    let raw = raw.trim();
    let quoted = raw.len() >= 2
        && ((raw.starts_with('"') && raw.ends_with('"'))
            || (raw.starts_with('\'') && raw.ends_with('\'')));
    let value = if quoted {
        raw[1..raw.len() - 1].to_string()
    } else {
        // Finder escapes spaces; leave Windows separators intact.
        raw.replace("\\ ", " ")
            .replace("\\(", "(")
            .replace("\\)", ")")
    };
    ensure!(!value.is_empty(), "KPI path cannot be empty");
    let path = if value == "~" || value.starts_with("~/") || value.starts_with("~\\") {
        dirs::home_dir()
            .context("Home directory unavailable")?
            .join(value.get(2..).unwrap_or(""))
    } else {
        PathBuf::from(value)
    };
    Ok(if path.is_absolute() {
        path
    } else {
        base.join(path)
    })
}

pub fn slug(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.truncate(60);
    out.trim_matches('-').to_string()
}

pub fn validate_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 80
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "ID must contain 1–80 ASCII letters, digits, - or _"
    );
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub integration: String,
    pub bitrix_user_id: String,
    #[serde(default, rename = "self")]
    pub is_self: bool,
    pub kpi: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    pub version: u32,
    #[serde(default)]
    pub people: Vec<Person>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plans: Vec<crate::plan::PlanReference>,
    #[serde(skip)]
    pub root: PathBuf,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalState {
    #[serde(default = "version")]
    pub version: u32,
    pub period: Option<String>,
    #[serde(default)]
    pub mappings: Vec<Mapping>,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub notes: Vec<String>,
}
fn version() -> u32 {
    1
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mapping {
    pub task_id: String,
    pub criterion: String,
    pub period: String,
    #[serde(default)]
    pub note: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLevel {
    Confirmed,
    Likely,
    Unknown,
    Negative,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub id: String,
    pub criterion: String,
    pub period: String,
    pub level: EvidenceLevel,
    pub source: String,
    pub note: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiDocument {
    pub path: String,
    pub content: Option<String>,
    pub status: String,
}
impl Workspace {
    pub fn discover(start: &Path) -> Result<Self> {
        let start = start
            .canonicalize()
            .context("Workspace directory unavailable")?;
        for parent in start.ancestors() {
            let root = if parent.file_name().is_some_and(|n| n == ".bkpi") {
                parent.to_path_buf()
            } else {
                parent.join(".bkpi")
            };
            if root.join("workspace.toml").exists() {
                return Self::load(&root);
            }
        }
        anyhow::bail!("No BKPI workspace. Run bkpi init in the project directory")
    }
    pub fn load(root: &Path) -> Result<Self> {
        ensure!(!root.is_symlink(), "Workspace root must not be a symlink");
        let root = root.canonicalize()?;
        let text = fs::read_to_string(root.join("workspace.toml"))?;
        let mut value: Self =
            toml::from_str(&text).map_err(|_| anyhow::anyhow!("Invalid workspace.toml"))?;
        value.root = root;
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "Unsupported workspace schema version");
        self.validate_plans()?;
        let mut ids = BTreeSet::new();
        ensure!(
            self.people.iter().filter(|p| p.is_self).count() <= 1,
            "Workspace can have at most one self person"
        );
        for p in &self.people {
            validate_id(&p.id)?;
            validate_id(&p.integration)?;
            ensure!(ids.insert(&p.id), "Duplicate person ID");
            ensure!(
                !p.name.trim().is_empty()
                    && !p.bitrix_user_id.is_empty()
                    && p.bitrix_user_id.bytes().all(|b| b.is_ascii_digit()),
                "Invalid person name or Bitrix user ID"
            );
            self.kpi_path(p)?;
        }
        Ok(())
    }
    pub fn safe_path(&self, relative: &str) -> Result<PathBuf> {
        let path = Path::new(relative);
        ensure!(
            !path.as_os_str().is_empty()
                && path.components().all(|c| matches!(c, Component::Normal(_))),
            "Path must stay inside .bkpi"
        );
        let mut current = self.root.clone();
        for part in path.components() {
            current.push(part);
            ensure!(
                !current.is_symlink(),
                "Symlinks inside workspace are not supported"
            );
        }
        Ok(current)
    }
    /// Legacy people/... references are rooted in .bkpi; all other relative
    /// references are project-relative. External documents are read-only here.
    pub fn kpi_path(&self, p: &Person) -> Result<PathBuf> {
        ensure!(!p.kpi.trim().is_empty(), "KPI path cannot be empty");
        if Path::new(&p.kpi).starts_with("people") {
            return self.safe_path(&p.kpi);
        }
        let project = self.root.parent().context("Workspace project missing")?;
        let path = if p.kpi == "~" || p.kpi.starts_with("~/") || p.kpi.starts_with("~\\") {
            input_path(&p.kpi, project)?
        } else {
            project.join(&p.kpi)
        };
        Ok(path.canonicalize().unwrap_or(path))
    }
    pub fn set_kpi(&mut self, id: &str, path: &Path) -> Result<()> {
        let path = path.canonicalize().context("KPI file not found")?;
        fs::read_to_string(&path).context("KPI must be a UTF-8 text file")?;
        let mut next = self.clone();
        next.people
            .iter_mut()
            .find(|p| p.id == id)
            .context("Person not found")?
            .kpi = path.to_string_lossy().into_owned();
        next.save()?;
        *self = next;
        Ok(())
    }
    pub fn visible_kpi(&self, p: &Person, single: bool) -> String {
        if single {
            return "KPI.md".into();
        }
        let name: String = p
            .name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();
        let name = name
            .split('-')
            .filter(|v| !v.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let name: String = name.chars().take(60).collect();
        let name = if name.is_empty() { p.id.clone() } else { name };
        let mut candidate = format!("kpi/{name}.md");
        let mut n = 2;
        while self.people.iter().any(|other| other.kpi == candidate)
            || self
                .root
                .parent()
                .is_some_and(|root| root.join(&candidate).exists())
        {
            candidate = format!("kpi/{name}-{n}.md");
            n += 1;
        }
        candidate
    }
    pub fn resolve(&self, id: Option<&str>) -> Result<&Person> {
        match id {
            Some("self") => self
                .people
                .iter()
                .find(|p| p.is_self)
                .context("No self person configured"),
            Some(id) => self
                .people
                .iter()
                .find(|p| p.id == id)
                .context("Person not found"),
            None if self.people.len() == 1 => Ok(&self.people[0]),
            None => self
                .people
                .iter()
                .find(|p| p.is_self)
                .context("Choose --person (multiple people and no self)"),
        }
    }
    pub fn save(&self) -> Result<()> {
        self.validate()?;
        private_write(
            &self.root.join("workspace.toml"),
            toml::to_string_pretty(self)?.as_bytes(),
        )
    }
    pub fn create(project: &Path) -> Result<Self> {
        let root = project.canonicalize()?.join(".bkpi");
        ensure!(!root.exists(), ".bkpi already exists; use bkpi person add");
        fs::create_dir(&root)?;
        let ws = Self {
            version: 1,
            people: vec![],
            plans: vec![],
            root,
        };
        ws.save()?;
        Ok(ws)
    }
    pub fn add(&mut self, person: Person) -> Result<()> {
        ensure!(
            !self.people.iter().any(|p| p.id == person.id),
            "Person already exists"
        );
        let mut next = self.clone();
        next.people.push(person.clone());
        next.validate()?;
        let kpi = next.kpi_path(&person)?;
        let state = next.state_path(&person)?;
        ensure!(
            !state.exists(),
            "Person files already exist; refusing to overwrite"
        );
        if !kpi.exists() {
            create_document(&kpi, KPI_TEMPLATE.as_bytes())?;
        } else {
            fs::read_to_string(&kpi).context("KPI must be a UTF-8 text file")?;
        }
        next.write_state(
            &person,
            &LocalState {
                version: 1,
                ..Default::default()
            },
        )?;
        next.save()?;
        *self = next;
        Ok(())
    }
    pub fn document(&self, p: &Person) -> Result<KpiDocument> {
        let path = self.kpi_path(p)?;
        let content = match fs::read_to_string(path) {
            Ok(t) => Some(t),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        let content = content.filter(|t| {
            !t.trim().is_empty()
                && t.trim() != KPI_TEMPLATE.trim()
                && t.trim() != LEGACY_KPI_TEMPLATE.trim()
                && t.trim() != "# KPI"
        });
        Ok(KpiDocument {
            path: self.kpi_path(p)?.display().to_string(),
            status: if content.is_some() {
                "available"
            } else {
                "missing"
            }
            .into(),
            content,
        })
    }
    fn state_path(&self, p: &Person) -> Result<PathBuf> {
        self.safe_path(&format!("people/{}/state.toml", p.id))
    }
    pub fn state(&self, p: &Person) -> Result<LocalState> {
        let path = self.state_path(p)?;
        if !path.exists() {
            return Ok(LocalState {
                version: 1,
                ..Default::default()
            });
        }
        let value: LocalState = toml::from_str(&fs::read_to_string(path)?)
            .map_err(|_| anyhow::anyhow!("Invalid state.toml"))?;
        ensure!(value.version == 1, "Unsupported state schema version");
        Ok(value)
    }
    fn write_state(&self, p: &Person, state: &LocalState) -> Result<()> {
        private_write(
            &self.state_path(p)?,
            toml::to_string_pretty(state)?.as_bytes(),
        )
    }
    pub fn update_state(
        &self,
        p: &Person,
        change: impl FnOnce(&mut LocalState) -> Result<()>,
    ) -> Result<LocalState> {
        let path = self.safe_path(&format!("people/{}/state.lock", p.id))?;
        fs::create_dir_all(path.parent().context("State parent missing")?)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)?;
        fs2::FileExt::lock_exclusive(&lock)?;
        let mut state = self.state(p)?;
        change(&mut state)?;
        self.write_state(p, &state)?;
        Ok(state)
    }
}
pub fn period(value: Option<&str>) -> Result<String> {
    let s = value
        .map(str::to_owned)
        .unwrap_or_else(|| chrono::Local::now().format("%Y-%m").to_string());
    ensure!(s.len() == 7, "Period must be YYYY-MM");
    chrono::NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("Period must be YYYY-MM"))?;
    Ok(s)
}

/// Create user-facing files exclusively: never truncate existing user data.
pub fn create_document(path: &Path, content: &[u8]) -> Result<()> {
    use std::io::Write;
    fs::create_dir_all(path.parent().context("KPI parent missing")?)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .context("KPI destination already exists or cannot be created")?;
    file.write_all(content)?;
    Ok(())
}
