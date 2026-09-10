use crate::{
    config::{Config, Integration},
    credentials::{Backend, CredentialRef, CredentialStore, webhook},
    snapshot::{Core, SnapshotOptions},
    workspace::{Person, Workspace},
};
use anyhow::{Context, Result, ensure};
use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    io::{self, IsTerminal, Write},
    path::PathBuf,
};

#[derive(Parser)]
#[command(version, about = "BKPI workspace tools for coding agents")]
pub struct Cli {
    #[arg(long, global = true)]
    pub workspace: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Option<Command>,
}
#[derive(Subcommand)]
pub enum Command {
    Init,
    /// Manage agreed period plan references (never downloads documents).
    Plan {
        #[command(subcommand)]
        command: PlanCommand,
    },
    Scoring {
        #[command(subcommand)]
        command: ScoringCommand,
    },
    /// Deterministic calculation; decimal fractions must be quoted strings.
    Calculate {
        #[arg(long, default_value = "-")]
        input_json: String,
        #[arg(long)]
        json: bool,
    },
    Integration {
        #[command(subcommand)]
        command: IntegrationCommand,
    },
    Person {
        #[command(subcommand)]
        command: PersonCommand,
    },
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Doctor,
    Tasks(TaskArgs),
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    Mcp,
    /// Generic workspace summary; the old profile dashboard was retired.
    Tui,
    Ai {
        #[command(subcommand)]
        command: AiCommand,
    },
}
#[derive(Subcommand)]
pub enum PlanCommand {
    List {
        #[arg(long)]
        json: bool,
    },
    Show {
        #[arg(long)]
        period: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Set {
        reference: String,
        #[arg(long)]
        period: String,
        #[arg(long)]
        label: Option<String>,
        #[arg(long, value_enum)]
        source: Option<crate::plan::PlanSource>,
        #[arg(long)]
        json: bool,
    },
    Clear {
        #[arg(long)]
        period: String,
        #[arg(long)]
        json: bool,
    },
}
#[derive(Subcommand)]
pub enum ScoringCommand {
    Show {
        #[arg(long)]
        json: bool,
    },
}
#[derive(Subcommand)]
pub enum WorkspaceCommand {
    Show,
}
#[derive(Subcommand)]
pub enum ConfigCommand {
    Show,
    Path,
    FixPermissions,
    Migrate {
        #[arg(long, conflicts_with = "backend")]
        file_fallback: bool,
        #[arg(long, value_enum, default_value = "file")]
        backend: Backend,
    },
}
#[derive(Subcommand)]
pub enum IntegrationCommand {
    List,
    Add {
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, conflicts_with = "backend")]
        file_fallback: bool,
        #[arg(long, value_enum)]
        backend: Option<Backend>,
        #[arg(long)]
        env_var: Option<String>,
        #[arg(long)]
        base_url: Option<String>,
    },
    Credential {
        #[command(subcommand)]
        command: CredentialCommand,
    },
    Remove {
        id: String,
    },
    Doctor {
        id: Option<String>,
    },
    Users {
        id: String,
        #[arg(long, default_value = "")]
        search: String,
    },
}
#[derive(Subcommand)]
pub enum CredentialCommand {
    Show {
        id: String,
        #[arg(long)]
        json: bool,
    },
    Move {
        id: String,
        #[arg(long, value_enum)]
        backend: Backend,
        #[arg(long)]
        env_var: Option<String>,
        #[arg(long)]
        confirm_env: bool,
    },
}
#[derive(Subcommand)]
pub enum PersonCommand {
    List,
    Add {
        #[arg(long)]
        integration: Option<String>,
        #[arg(long)]
        user_id: Option<String>,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long = "self")]
        is_self: bool,
    },
    Remove {
        id: String,
    },
    Kpi {
        #[command(subcommand)]
        command: KpiCommand,
    },
}
#[derive(Subcommand)]
pub enum KpiCommand {
    /// Point at an existing UTF-8 document; omit person in a single-person workspace.
    Set {
        #[arg(value_name = "PERSON_OR_PATH")]
        person_or_path: String,
        path: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Show {
        person: Option<String>,
        #[arg(long)]
        json: bool,
    },
}
#[derive(Args, Clone)]
pub struct SnapshotArgs {
    #[arg(long)]
    pub person: Option<String>,
    #[arg(long)]
    pub month: Option<String>,
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
    #[arg(long)]
    pub json: bool,
}
#[derive(Args)]
pub struct TaskArgs {
    #[command(flatten)]
    pub snapshot: SnapshotArgs,
    #[arg(long, conflicts_with = "closed")]
    pub open: bool,
    #[arg(long, conflicts_with = "open")]
    pub closed: bool,
    #[arg(long)]
    pub late: bool,
}
#[derive(Subcommand)]
pub enum AgentCommand {
    Snapshot(SnapshotArgs),
    TeamSnapshot {
        #[arg(long, value_delimiter = ',')]
        person_ids: Vec<String>,
        #[arg(long)]
        month: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Shell fallback for any MCP tool; JSON arguments must contain no credentials.
    Call {
        tool: String,
        #[arg(long, default_value = "{}")]
        args: String,
    },
}
#[derive(Subcommand)]
pub enum AiCommand {
    Setup {
        #[arg(long, default_value = "openrouter/auto")]
        model: String,
        #[arg(long, conflicts_with = "backend")]
        file_fallback: bool,
        #[arg(long, value_enum)]
        backend: Option<Backend>,
        #[arg(long)]
        env_var: Option<String>,
    },
    Doctor,
    Analyze(SnapshotArgs),
}
fn interactive() -> bool {
    io::stdin().is_terminal() && io::stderr().is_terminal()
}
fn optional_prompt(message: &str) -> Result<String> {
    if interactive() {
        return Ok(dialoguer::Input::<String>::with_theme(
            &dialoguer::theme::ColorfulTheme::default(),
        )
        .with_prompt(message)
        .allow_empty(true)
        .report(true)
        .interact_text()?);
    }
    eprint!("◇ {message}: ");
    io::stderr().flush()?;
    let mut line = String::new();
    ensure!(io::stdin().read_line(&mut line)? > 0, "Input cancelled");
    Ok(line.trim().to_string())
}
fn prompt(message: &str) -> Result<String> {
    let value = optional_prompt(message)?;
    ensure!(!value.is_empty(), "Input cannot be empty");
    Ok(value)
}
fn confirm(message: &str, default: bool) -> Result<bool> {
    if interactive() {
        return Ok(
            dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
                .with_prompt(message)
                .default(default)
                .interact()?,
        );
    }
    loop {
        let answer = optional_prompt(&format!(
            "{message} {}",
            if default { "[Y/n]" } else { "[y/N]" }
        ))?;
        match answer.to_lowercase().as_str() {
            "" => return Ok(default),
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => eprintln!("Enter yes or no."),
        }
    }
}
fn select(message: &str, labels: &[String]) -> Result<usize> {
    ensure!(!labels.is_empty(), "No choices available");
    if interactive() {
        return Ok(
            dialoguer::Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
                .with_prompt(message)
                .items(labels)
                .default(0)
                .interact()?,
        );
    }
    for (index, label) in labels.iter().enumerate() {
        eprintln!("  {}. {label}", index + 1);
    }
    let answer = prompt(message)?;
    let index = answer
        .parse::<usize>()
        .ok()
        .and_then(|i| i.checked_sub(1))
        .or_else(|| {
            labels
                .iter()
                .position(|label| label.eq_ignore_ascii_case(&answer))
        })
        .context("Choose an item number")?;
    ensure!(index < labels.len(), "Choice out of range");
    Ok(index)
}
fn kpi_person(ws: &Workspace, id: Option<&str>) -> Result<String> {
    ensure!(
        id.is_some() || ws.people.len() == 1,
        "Choose a person for this multi-person workspace"
    );
    Ok(ws.resolve(id)?.id.clone())
}
fn show_kpi(ws: &Workspace, id: &str, as_json: bool) -> Result<()> {
    let p = ws.resolve(Some(id))?;
    let path = ws.kpi_path(p)?;
    let empty = ws.document(p)?.content.is_none();
    if as_json {
        print_json(&json!({"person":p,"path":path,"exists":path.exists(),"empty":empty}))?;
    } else {
        println!(
            "Person: {}\nKPI document: {}\nExists: {}\nContent: {}",
            p.name,
            path.display(),
            path.exists(),
            if empty { "empty" } else { "non-empty" }
        );
    }
    Ok(())
}
fn workspace_summary(ws: &Workspace) -> Result<()> {
    eprintln!("◆ Workspace created");
    for p in &ws.people {
        let path = ws.kpi_path(p)?;
        eprintln!(
            "\nPerson:\n  {}\n\nKPI document:\n  {}",
            p.name,
            path.display()
        );
        if ws.document(p)?.content.is_none() {
            eprintln!(
                "\nKPI document is currently empty.\n\nYou can:\n1. edit:\n   {}\n\n2. or paste/attach KPI directly in the agent chat for a one-off analysis.",
                path.display()
            );
        }
    }
    eprintln!(
        "\nOpen this directory in Codex/Claude:\n  {}\nand ask:\n  \"Проанализируй мои KPI и задачи в Bitrix.\"",
        ws.root.parent().context("Project missing")?.display()
    );
    Ok(())
}
fn print_json(value: &impl serde::Serialize) -> Result<()> {
    let mut value = serde_json::to_value(value)?;
    crate::credentials::redact_value(&mut value);
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
fn current(start: &Option<PathBuf>) -> Result<Workspace> {
    Workspace::discover(&start.clone().unwrap_or(std::env::current_dir()?))
}
async fn doctor(id: Option<&str>) -> Result<()> {
    let config = Config::load()?;
    ensure!(
        !config.integrations.is_empty(),
        "No integrations; run bkpi integration add"
    );
    for i in config
        .integrations
        .iter()
        .filter(|i| id.is_none_or(|id| i.id == id))
    {
        let store = Config::store()?;
        eprintln!(
            "✓ integration = {}\n✓ credential backend = {}",
            crate::credentials::redact_text(&i.display_name),
            i.credential.backend().name()
        );
        store.load(&i.credential)?;
        eprintln!("✓ credential configured");
        if i.credential.backend() == Backend::File {
            store.check_permissions(&i.credential)?;
            #[cfg(unix)]
            eprintln!("✓ credential file permissions = secure");
            #[cfg(not(unix))]
            eprintln!("Credential file uses user directory ACLs; Unix mode check unavailable");
        }
        let c = config.client(&i.id)?;
        let u = c.current_user().await?;
        let tasks = c.tasks_for_user(&u.id).await?;
        eprintln!(
            "✓ Bitrix reachable\n✓ user = {} ({})\n✓ task access works: {} tasks",
            crate::credentials::redact_text(&u.display_name()),
            u.id,
            tasks.len()
        );
    }
    if let Some(id) = id {
        config.integration(id)?;
    }
    Ok(())
}
async fn add_person(
    ws: &mut Workspace,
    integration: Option<String>,
    user_id: Option<String>,
    id: Option<String>,
    name: Option<String>,
    is_self: bool,
    ask_kpi: bool,
) -> Result<()> {
    let config = Config::load()?;
    ensure!(
        !config.integrations.is_empty(),
        "Run bkpi integration add first"
    );
    let integration = match integration {
        Some(v) => v,
        None if config.integrations.len() == 1 => config.integrations[0].id.clone(),
        None => {
            let labels: Vec<_> = config
                .integrations
                .iter()
                .map(|i| i.display_name.clone())
                .collect();
            config.integrations[select("Bitrix integration", &labels)?]
                .id
                .clone()
        }
    };
    eprintln!(
        "◇ Bitrix integration\n│ {}",
        config.integration(&integration)?.display_name
    );
    let client = config.client(&integration)?;
    let me = client.current_user().await?;
    let users = if let Some(user_id) = user_id {
        vec![crate::bitrix::User {
            id: user_id,
            name: name.unwrap_or_else(|| "Person".into()),
            last_name: String::new(),
            email: String::new(),
        }]
    } else if is_self {
        vec![me.clone()]
    } else {
        let choice = if interactive() {
            select(
                "Who should be added?",
                &["Me".into(), "Search employees".into()],
            )?
        } else {
            match prompt("Who should be added? [me/search]")?.as_str() {
                "me" => 0,
                "search" => 1,
                _ => anyhow::bail!("Choose me or search"),
            }
        };
        if choice == 0 {
            vec![me.clone()]
        } else {
            let search = prompt("Employee name search")?;
            let users = client.users(&search).await?;
            ensure!(!users.is_empty(), "No employees found; try another search");
            if interactive() {
                let labels: Vec<_> = users.iter().map(|u| u.display_name()).collect();
                dialoguer::MultiSelect::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .with_prompt("Choose employees (Space to select, Enter to continue)")
                    .items(&labels)
                    .interact()?
                    .into_iter()
                    .map(|i| users[i].clone())
                    .collect::<Vec<_>>()
            } else {
                for u in &users {
                    eprintln!("{} — {}", u.id, u.display_name());
                }
                let ids = prompt("Select user IDs, comma separated")?;
                ids.split(',')
                    .map(str::trim)
                    .map(|id| {
                        users
                            .iter()
                            .find(|u| u.id == id)
                            .cloned()
                            .context("User ID not in search results")
                    })
                    .collect::<Result<Vec<_>>>()?
            }
        }
    };
    ensure!(!users.is_empty(), "No users selected");
    let single = ws.people.len() + users.len() == 1;
    let mut planned = ws.clone();
    let mut documents = Vec::new();
    for user in users {
        let person_id = id
            .clone()
            .unwrap_or_else(|| format!("{}-{}", integration, user.id));
        let mut person = Person {
            id: person_id,
            name: user.display_name(),
            integration: integration.clone(),
            bitrix_user_id: user.id.clone(),
            is_self: is_self || (user.id == me.id && !planned.people.iter().any(|p| p.is_self)),
            kpi: String::new(),
        };
        person.kpi = planned.visible_kpi(&person, single);
        eprintln!("◇ Person\n│ {}", person.name);
        let raw = if ask_kpi {
            optional_prompt("Do you already have a KPI document? (path, or Enter to skip)")?
        } else {
            String::new()
        };
        let mut content = None;
        if !raw.is_empty() {
            let source = crate::workspace::input_path(&raw, &std::env::current_dir()?)?
                .canonicalize()
                .context("KPI file not found")?;
            let text = std::fs::read_to_string(&source)
                .context("KPI must be a UTF-8 text file (Markdown or plain text)")?;
            if confirm("Copy KPI document into this workspace?", true)? {
                // Text is read as UTF-8, so Markdown is a safe destination extension.
                let destination = planned.kpi_path(&person)?;
                if source != destination {
                    ensure!(
                        !destination.exists(),
                        "KPI destination exists; choose a reference instead of overwriting it"
                    );
                    content = Some(text);
                }
            } else {
                person.kpi = source.to_string_lossy().into_owned();
            }
        }
        // Reuse an existing visible document without changing its contents.
        let destination = planned.kpi_path(&person)?;
        if destination.exists() {
            std::fs::read_to_string(&destination).context("KPI must be a UTF-8 text file")?;
        }
        planned.people.push(person.clone());
        planned.validate()?;
        documents.push((person, content));
    }
    // Cancellation/input validation above never leaves a half-created workspace.
    if !ws.root.exists() {
        *ws = Workspace::create(ws.root.parent().context("Project missing")?)?;
    }
    for (person, content) in documents {
        if let Some(text) = content {
            crate::workspace::create_document(&ws.kpi_path(&person)?, text.as_bytes())?;
        }
        ws.add(person)?;
    }
    Ok(())
}
pub fn env_instruction(name: &str) -> Result<()> {
    crate::credentials::validate_env(name)?;
    eprintln!(
        "Set {name} in the environment of the BKPI/agent process using your CI secret settings or a masked shell prompt. BKPI does not save its value or modify shell profiles."
    );
    #[cfg(unix)]
    eprintln!(
        "Bash (hidden input; value never enters command history): read -r -s -p 'Credential: ' {name}; export {name}"
    );
    #[cfg(windows)]
    eprintln!(
        "PowerShell: $credential = Read-Host 'Credential' -AsSecureString; [Environment]::SetEnvironmentVariable('{name}', [System.Net.NetworkCredential]::new('', $credential).Password, 'Process')"
    );
    Ok(())
}
fn choose_backend(
    existing: Option<Backend>,
    explicit: Option<Backend>,
    file_fallback: bool,
) -> Result<Backend> {
    if file_fallback {
        return Ok(Backend::File);
    }
    if let Some(backend) = explicit.or(existing) {
        return Ok(backend);
    }
    if !interactive() {
        return Ok(Backend::File);
    }
    Ok([Backend::File, Backend::Keychain, Backend::Env][select(
        "Where should BKPI store credentials?",
        &[
            "Private local file — Simple and portable. Owner-only permissions.".into(),
            "OS secure credential store — Keychain / Credential Manager / Secret Service.".into(),
            "Environment variables — Advanced / CI usage.".into(),
        ],
    )?])
}
async fn add_integration(
    id: Option<String>,
    name: Option<String>,
    file_fallback: bool,
    backend: Option<Backend>,
    env_var: Option<String>,
    origin: Option<String>,
) -> Result<()> {
    let mut config = Config::load()?;
    let existing = id
        .as_deref()
        .and_then(|id| config.integrations.iter().find(|i| i.id == id))
        .cloned();
    let backend = choose_backend(
        existing.as_ref().map(|i| i.credential.backend()),
        backend,
        file_fallback,
    )?;
    if let Some(i) = &existing {
        ensure!(
            backend == i.credential.backend(),
            "Use bkpi integration credential move to change storage before updating the webhook"
        );
    }
    ensure!(
        backend == Backend::Env || (env_var.is_none() && origin.is_none()),
        "--env-var and --base-url apply only to env storage"
    );
    let store = Config::store()?;
    let (secret, base_url, env_reference) = if backend == Backend::Env {
        let base = origin
            .or_else(|| existing.as_ref().map(|i| i.base_url.clone()))
            .map(Ok)
            .unwrap_or_else(|| prompt("Bitrix HTTPS origin (no webhook path)"))?;
        let url = url::Url::parse(&base).map_err(|_| anyhow::anyhow!("Invalid Bitrix origin"))?;
        ensure!(
            url.origin().ascii_serialization() == base && url.scheme() == "https",
            "Use only the HTTPS origin, without a webhook path"
        );
        let generated_id = id
            .clone()
            .map(Ok)
            .unwrap_or_else(|| crate::config::integration_id(&base, &config.integrations))?;
        let variable = env_var
            .or_else(|| existing.as_ref().map(|i| i.credential.account().into()))
            .unwrap_or_else(|| {
                format!(
                    "BKPI_BITRIX_{}_WEBHOOK",
                    generated_id.replace('-', "_").to_uppercase()
                )
            });
        env_instruction(&variable)?;
        let reference = CredentialRef::Env { account: variable };
        // Missing env is an explicit pending setup; malformed present values fail validation.
        let secret = match store.load(&reference) {
            Ok(secret) => {
                let (_, actual) = webhook(secret.expose())?;
                ensure!(
                    actual == base,
                    "Environment webhook belongs to another origin"
                );
                Some(secret)
            }
            Err(_) => None,
        };
        (secret, base, Some(reference))
    } else {
        let raw = if io::stdin().is_terminal() {
            rpassword::prompt_password("◇ Bitrix incoming webhook (hidden): ")
                .context("Cannot read masked credential")?
        } else {
            prompt("Bitrix incoming webhook (stdin; not echoed)")?
        };
        let (secret, base) = webhook(&raw)?;
        (Some(secret), base, None)
    };
    let id = id
        .map(Ok)
        .unwrap_or_else(|| crate::config::integration_id(&base_url, &config.integrations))?;
    crate::workspace::validate_id(&id)?;
    let name = name
        .or_else(|| existing.as_ref().map(|i| i.display_name.clone()))
        .map(Ok)
        .unwrap_or_else(|| optional_prompt("Integration display name (optional)"))?;
    let name = if name.trim().is_empty() {
        url::Url::parse(&base_url)?
            .host_str()
            .context("Hostname missing")?
            .to_string()
    } else {
        name
    };
    let verified = if let Some(secret) = &secret {
        let client = crate::bitrix::BitrixClient::new(secret.expose())?;
        let user = client.current_user().await?;
        let tasks = client.tasks_for_user(&user.id).await?;
        Some((secret.redact(&user.display_name()), user.id, tasks.len()))
    } else {
        None
    };
    let credential = if let Some(reference) = env_reference {
        reference
    } else {
        let account = if existing.is_some() {
            let suffix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos();
            format!("bitrix-{id}-{suffix}")
        } else {
            format!("bitrix-{id}")
        };
        let secret = secret.as_ref().context("Credential missing")?;
        let reference = store.set(&account, secret, backend)?;
        ensure!(
            store.load(&reference)?.expose() == secret.expose(),
            "Credential verification failed; configuration unchanged"
        );
        reference
    };
    config.integrations.retain(|i| i.id != id);
    config.integrations.push(Integration {
        id,
        display_name: name,
        base_url,
        credential,
    });
    config.save()?;
    if let Some(previous) = existing {
        let shared = config
            .integrations
            .iter()
            .any(|i| i.credential == previous.credential)
            || config
                .openrouter
                .as_ref()
                .is_some_and(|ai| ai.credential == previous.credential);
        if !shared && backend != Backend::Env && store.remove(&previous.credential).is_err() {
            eprintln!("Warning: new credential saved; previous credential could not be removed.");
        }
    }

    if backend != Backend::Env {
        eprintln!("✓ Credential saved securely");
        #[cfg(unix)]
        if backend == Backend::File {
            eprintln!("✓ File permissions: owner only");
        }
    }
    if let Some((name, id, count)) = verified {
        eprintln!("✓ {name} ({id})\n✓ Task access: {count} tasks");
    } else {
        eprintln!(
            "Integration saved but NOT usable until the environment variable is set. Run bkpi integration doctor afterwards."
        );
    }
    Ok(())
}
pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Some(Command::Plan { command }) => {
            let mut ws = current(&cli.workspace)?;
            match command {
                PlanCommand::List { .. } => print_json(&json!({"plans":ws.plans}))?,
                PlanCommand::Show { period: month, .. } => {
                    let month = match month {
                        Some(month) => crate::workspace::period(Some(&month))?,
                        None if ws.plans.len() == 1 => {
                            ws.plans[0].start_date.format("%Y-%m").to_string()
                        }
                        None => crate::workspace::period(None)?,
                    };
                    print_json(&json!({"period":month,"plans":ws.plans_for(&month)?}))?;
                }
                PlanCommand::Set {
                    reference,
                    period,
                    label,
                    source,
                    ..
                } => {
                    let existing = ws.plans.iter().find(|p| p.id == period);
                    let label = label.or_else(|| existing.map(|p| p.label.clone()));
                    let mut plan =
                        crate::plan::PlanReference::for_month(&period, reference, source, label)?;
                    if let Some(existing) = existing {
                        plan.person_sections = existing.person_sections.clone();
                    }
                    ws.set_plan(plan)?;
                    print_json(&json!({"plans":ws.plans_for(&period)?}))?;
                }
                PlanCommand::Clear { period, .. } => {
                    ws.clear_plan(&period)?;
                    print_json(&json!({"plans":ws.plans}))?;
                }
            }
        }
        Some(Command::Scoring { .. }) => print_json(&current(&cli.workspace)?.scoring_info()?)?,
        Some(Command::Calculate { input_json, .. }) => {
            use std::io::Read;
            let mut bytes = Vec::new();
            if input_json == "-" {
                io::stdin().take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
            } else {
                std::fs::File::open(input_json)
                    .context("Cannot open calculation input")?
                    .take(1024 * 1024 + 1)
                    .read_to_end(&mut bytes)?;
            }
            ensure!(
                bytes.len() <= 1024 * 1024,
                "Calculation input exceeds 1 MiB"
            );
            let request = serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("Invalid calculation input: use criterion IDs, integer money-free inputs and quoted decimal fractions"))?;
            print_json(&current(&cli.workspace)?.calculate(request)?)?;
        }
        Some(Command::Mcp) => return crate::mcp::serve(cli.workspace).await,
        Some(Command::Init) => {
            let dir = cli.workspace.unwrap_or(std::env::current_dir()?);
            if dir.join(".bkpi").exists() {
                anyhow::bail!("Workspace exists; use bkpi person add");
            }
            let dir = dir
                .canonicalize()
                .context("Workspace directory unavailable")?;
            eprintln!("┌ BKPI workspace\n│ {}", dir.display());
            ensure!(confirm("Create workspace here?", false)?, "Cancelled");
            let mut ws = Workspace {
                version: 1,
                people: vec![],
                plans: vec![],
                root: dir.join(".bkpi"),
            };
            add_person(&mut ws, None, None, None, None, false, true).await?;
            workspace_summary(&ws)?;
        }
        Some(Command::Config { command }) => match command {
            ConfigCommand::Show => print_json(&Config::load()?)?,
            ConfigCommand::Path => println!("{}", Config::root()?.join("config.toml").display()),
            ConfigCommand::FixPermissions => {
                Config::store()?.fix_permissions()?;
                eprintln!("✓ Credential file permissions repaired");
            }
            ConfigCommand::Migrate {
                file_fallback,
                backend,
            } => {
                ensure!(
                    backend != Backend::Env,
                    "Migrate into file or keychain first, then explicitly move to env"
                );
                crate::config::migrate(
                    &Config::root()?,
                    &Config::store()?,
                    file_fallback || backend == Backend::File,
                )?;
            }
        },
        Some(Command::Integration { command }) => match command {
            IntegrationCommand::List => print_json(&Config::load()?.integrations)?,
            IntegrationCommand::Add {
                id,
                name,
                file_fallback,
                backend,
                env_var,
                base_url,
            } => {
                add_integration(id, name, file_fallback, backend, env_var, base_url).await?;
            }
            IntegrationCommand::Credential { command } => match command {
                CredentialCommand::Show { id, json: as_json } => {
                    let config = Config::load()?;
                    let i = config.integration(&id)?;
                    let configured = Config::store()?.exists(&i.credential);
                    if as_json {
                        print_json(
                            &json!({"integration":i.display_name,"backend":i.credential.backend().name(),"reference":i.credential.account(),"configured":configured}),
                        )?;
                    } else {
                        eprintln!(
                            "Integration: {}\nBackend: {}\nReference: {}\nConfigured: {}",
                            crate::credentials::redact_text(&i.display_name),
                            i.credential.backend().name(),
                            i.credential.account(),
                            if configured { "yes" } else { "no" }
                        );
                    }
                }
                CredentialCommand::Move {
                    id,
                    backend,
                    env_var,
                    confirm_env,
                } => {
                    if backend == Backend::Env {
                        env_instruction(env_var.as_deref().context("Use --env-var NAME")?)?;
                    }
                    let cleanup_failed = crate::config::move_credential(
                        &Config::root()?,
                        &Config::store()?,
                        &id,
                        backend,
                        env_var.as_deref(),
                        confirm_env,
                    )?;
                    eprintln!("✓ Credential reference saved");
                    if backend == Backend::Env {
                        eprintln!(
                            "Previous credential retained for recovery; ensure the variable is available to each agent host."
                        );
                    }
                    if cleanup_failed {
                        eprintln!(
                            "Warning: old credential could not be deleted; the new credential is working."
                        );
                    }
                }
            },
            IntegrationCommand::Remove { id } => {
                let mut c = Config::load()?;
                let reference = c.integration(&id)?.credential.clone();
                c.integrations.retain(|i| i.id != id);
                c.save()?;
                Config::store()?.remove(&reference)?;
                eprintln!(
                    "Removed integration. Workspaces referencing it need another integration."
                );
            }
            IntegrationCommand::Doctor { id } => doctor(id.as_deref()).await?,
            IntegrationCommand::Users { id, search } => {
                let c = Config::load()?.client(&id)?;
                print_json(&c.sanitize(json!({"users":c.users(&search).await?}))?)?;
            }
        },
        Some(Command::Doctor) => doctor(None).await?,
        Some(Command::Person { command }) => {
            let mut ws = current(&cli.workspace)?;
            match command {
                PersonCommand::List => print_json(&ws.people)?,
                PersonCommand::Kpi { command } => match command {
                    KpiCommand::Show { person, json } => {
                        let id = kpi_person(&ws, person.as_deref())?;
                        show_kpi(&ws, &id, json)?;
                    }
                    KpiCommand::Set {
                        person_or_path,
                        path,
                        json,
                    } => {
                        let (person, raw) = match path {
                            Some(path) => (Some(person_or_path), path),
                            None => (None, person_or_path),
                        };
                        let id = kpi_person(&ws, person.as_deref())?;
                        let path = crate::workspace::input_path(&raw, &std::env::current_dir()?)?;
                        ws.set_kpi(&id, &path)?;
                        show_kpi(&ws, &id, json)?;
                    }
                },
                PersonCommand::Add {
                    integration,
                    user_id,
                    id,
                    name,
                    is_self,
                } => {
                    add_person(
                        &mut ws,
                        integration,
                        user_id,
                        id,
                        name,
                        is_self,
                        interactive(),
                    )
                    .await?
                }
                PersonCommand::Remove { id } => {
                    let id = ws.resolve(Some(&id))?.id.clone();
                    ws.people.retain(|p| p.id != id);
                    for plan in &mut ws.plans {
                        plan.person_sections.remove(&id);
                    }
                    ws.save()?;
                    eprintln!(
                        "Person removed from workspace; KPI/state files retained for recovery."
                    );
                }
            }
        }
        Some(Command::Tasks(args)) => {
            let mut core = Core::new(current(&cli.workspace)?);
            let p = core
                .workspace
                .resolve(args.snapshot.person.as_deref())?
                .clone();
            core.prepare(std::slice::from_ref(&p))?;
            let tasks = core.tasks(&p, args.snapshot.month.as_deref()).await?;
            let selected: Vec<_> = tasks
                .iter()
                .filter(|t| {
                    (!args.open || !t.is_closed())
                        && (!args.closed || t.is_closed())
                        && (!args.late
                            || (!t.is_closed()
                                && t.deadline
                                    .as_deref()
                                    .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
                                    .is_some_and(|d| d < chrono::Utc::now())))
                })
                .take(args.snapshot.limit)
                .collect();
            let value = core
                .client(&p)?
                .sanitize(json!({"person":p,"tasks":selected}))?;
            print_json(&value)?;
        }
        Some(Command::Agent { command }) => match command {
            AgentCommand::Snapshot(a) => {
                let mut core = Core::new(current(&cli.workspace)?);
                let p = core.workspace.resolve(a.person.as_deref())?.clone();
                core.prepare(std::slice::from_ref(&p))?;
                print_json(
                    &core
                        .snapshot(
                            &p,
                            &SnapshotOptions {
                                month: a.month,
                                limit: Some(a.limit),
                            },
                        )
                        .await?,
                )?;
            }
            AgentCommand::TeamSnapshot {
                person_ids,
                month,
                limit,
                ..
            } => {
                let mut core = Core::new(current(&cli.workspace)?);
                let people = core.select_people(&person_ids)?;
                core.prepare_team(&people);
                print_json(
                    &core
                        .team(
                            &person_ids,
                            &SnapshotOptions {
                                month,
                                limit: Some(limit),
                            },
                        )
                        .await?,
                )?;
            }
            AgentCommand::Call { tool, args } => {
                let mut args: Value = serde_json::from_str(&args)
                    .map_err(|_| anyhow::anyhow!("Invalid tool arguments JSON"))?;
                if let Some(ref path) = cli.workspace {
                    args["workspace"] = json!(path);
                }
                print_json(&crate::mcp::call(&tool, args, cli.workspace.as_deref()).await?)?;
            }
        },
        Some(Command::Ai { command }) => {
            #[cfg(feature = "standalone-ai")]
            crate::ai::run(command, cli.workspace).await?;
            #[cfg(not(feature = "standalone-ai"))]
            {
                let _ = command;
                anyhow::bail!(
                    "Standalone AI is optional; use an agent with MCP, or a runtime built with --features standalone-ai"
                );
            }
        }
        None | Some(Command::Tui) | Some(Command::Workspace { .. }) => print_json(
            &json!({"workspace":current(&cli.workspace)?,"analysis":"Use a coding agent with your KPI document. No built-in KPI scoring."}),
        )?,
    }
    Ok(())
}
