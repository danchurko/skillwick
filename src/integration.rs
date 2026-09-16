use crate::{
    config::{self, Agent, Config, Discovery},
    inventory,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::Command,
};

const CONTEXT: &str = include_str!("../assets/skillwick/SKILLWICK.md");
const JOURNAL_VERSION: u32 = 3;
const TRANSACTION_VERSION: u32 = 1;

pub fn instructions() -> &'static str {
    CONTEXT
}

#[derive(Clone, Debug)]
pub struct InitRequest {
    pub cwd: PathBuf,
    pub yes: bool,
    pub dry_run: bool,
    pub agents: Vec<Agent>,
    pub roots: Vec<PathBuf>,
    pub project_roots: Vec<PathBuf>,
    pub instructions_file: Option<PathBuf>,
    pub discovery: Option<Discovery>,
    pub project: bool,
}

#[derive(Debug)]
pub enum Error {
    Usage(String),
    Operational(String),
}

impl Error {
    pub fn code(&self) -> i32 {
        match self {
            Self::Usage(_) => 2,
            Self::Operational(_) => 1,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(error) | Self::Operational(error) => formatter.write_str(error),
        }
    }
}

impl From<String> for Error {
    fn from(error: String) -> Self {
        Self::Operational(error)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u32,
    #[serde(default)]
    targets: Vec<JournalTarget>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct JournalTarget {
    agent: Agent,
    #[serde(default)]
    project: Option<PathBuf>,
    instructions_file: PathBuf,
    context_file: PathBuf,
    reference: String,
    #[serde(default)]
    reference_added: bool,
    #[serde(default)]
    instructions_created: bool,
    context_hash: String,
    #[serde(default)]
    context_created: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Transaction {
    version: u32,
    committed: bool,
    changes: Vec<Change>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Change {
    path: PathBuf,
    before: Option<Vec<u8>>,
    after: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
struct TargetPaths {
    agent: Agent,
    project: Option<PathBuf>,
    instructions_file: PathBuf,
    context_file: PathBuf,
    reference: String,
}

#[derive(Clone, Debug)]
struct SetupPlan {
    settings: Config,
    changes: Vec<Change>,
    summary: Vec<String>,
}

struct SetupLock {
    path: PathBuf,
}

impl Drop for SetupLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn init(config_path: &Path, request: InitRequest) -> Result<Config, Error> {
    let destination = absolute_destination(config_path)?;
    let config_path = destination.as_path();
    if !request.dry_run && !interactive() && (!request.yes || request.agents.is_empty()) {
        return Err(Error::Usage(
            "non-interactive init requires --yes and an explicit --agent target".into(),
        ));
    }
    if request.dry_run {
        if fs::symlink_metadata(pending_path()).is_ok() {
            report_pending(&format!(
                "pending Skillwick setup transaction at {}; dry-run leaves it unchanged",
                pending_path().display()
            ))?;
        }
        let settings = config::load(config_path).map_err(Error::Operational)?;
        let selected = select_agents(&settings, &request)?;
        let settings = merge_settings(settings, &request, &selected)?;
        let journal = read_journal()?.unwrap_or(Journal {
            version: JOURNAL_VERSION,
            targets: Vec::new(),
        });
        let plan = build_plan(config_path, &request, settings, selected, journal)?;
        print_plan(&plan)?;
        return Ok(plan.settings);
    }

    // Recover an interrupted write before planning. Planning inspects current
    // destinations for ownership collisions, so a partial transaction must be
    // rolled back before those checks run.
    let _lock = acquire_lock()?;
    recover_pending()?;
    let settings = config::load(config_path).map_err(Error::Operational)?;
    let selected = select_agents(&settings, &request)?;
    let settings = merge_settings(settings, &request, &selected)?;
    let journal = read_journal()?.unwrap_or(Journal {
        version: JOURNAL_VERSION,
        targets: Vec::new(),
    });
    let plan = build_plan(config_path, &request, settings, selected, journal)?;
    print_plan(&plan)?;
    if !request.yes {
        confirm()?;
    }

    // Setup owns reconciliation so an applied configuration is immediately usable.
    inventory::prime(&plan.settings, &request.cwd)
        .map_err(|error| Error::Operational(error.to_string()))?;
    apply_plan(&plan)
}

fn select_agents(settings: &Config, request: &InitRequest) -> Result<Vec<Agent>, Error> {
    let mut agents = if request.agents.is_empty() {
        if request.yes || request.dry_run || !interactive() {
            return Err(Error::Usage(
                "non-interactive init requires at least one --agent target; use --agent none to configure discovery only"
                    .into(),
            ));
        } else if !settings.agents.is_empty() {
            settings.agents.clone()
        } else {
            detected_agents()
        }
    } else {
        request.agents.clone()
    };
    agents.sort_by_key(|agent| agent_key(*agent));
    agents.dedup();
    if agents.is_empty() {
        return Err(Error::Usage(
            "init found no supported agent targets; pass --agent codex or --agent claude".into(),
        ));
    }
    if agents.contains(&Agent::None) && agents.len() != 1 {
        return Err(Error::Usage(
            "--agent none cannot be combined with other targets".into(),
        ));
    }
    Ok(agents)
}

fn detected_agents() -> Vec<Agent> {
    let mut agents = Vec::new();
    if config::codex_home().is_dir() {
        agents.push(Agent::Codex);
    }
    if config::claude_home().is_dir() {
        agents.push(Agent::Claude);
    }
    agents
}

fn merge_settings(
    mut settings: Config,
    request: &InitRequest,
    agents: &[Agent],
) -> Result<Config, Error> {
    settings.agents = agents
        .iter()
        .copied()
        .filter(|agent| *agent != Agent::None)
        .collect();
    if let Some(discovery) = request.discovery.filter(|_| !request.project) {
        settings.discovery = discovery;
    }
    if !request.roots.is_empty() {
        settings.discovery = request.discovery.unwrap_or(Discovery::Explicit);
        for root in &request.roots {
            let root = normalize_root(root)?;
            if !settings.roots.contains(&root) {
                settings.roots.push(root);
            }
        }
    }

    if request.project || !request.project_roots.is_empty() {
        let path = config::normalize_cwd(&request.cwd).map_err(Error::Usage)?;
        let position = settings.projects.iter().position(|project| {
            fs::canonicalize(&project.path)
                .ok()
                .is_some_and(|existing| existing == path)
        });
        let project = if let Some(position) = position {
            &mut settings.projects[position]
        } else {
            settings.projects.push(config::Project {
                path: path.clone(),
                roots: Vec::new(),
                discovery: Discovery::Auto,
            });
            settings.projects.last_mut().expect("project was pushed")
        };
        if let Some(discovery) = request.discovery {
            project.discovery = discovery;
        } else if !request.project_roots.is_empty() {
            project.discovery = Discovery::Explicit;
        }
        for root in &request.project_roots {
            let root = normalize_root(root)?;
            if !project.roots.contains(&root) {
                project.roots.push(root);
            }
        }
    } else if let Some(Discovery::Explicit) = request.discovery {
        settings.discovery = Discovery::Explicit;
    }
    if let Some(path) = &request.instructions_file {
        settings.instructions_file = Some(absolute_destination(path)?);
    }
    validate_discovery(&settings, request.project, &request.cwd)?;
    Ok(settings)
}

fn validate_discovery(settings: &Config, project: bool, cwd: &Path) -> Result<(), Error> {
    if settings.discovery == Discovery::Explicit && settings.roots.is_empty() && !project {
        return Err(Error::Usage(
            "explicit discovery requires at least one --root".into(),
        ));
    }
    if project {
        let path = config::normalize_cwd(cwd).map_err(Error::Usage)?;
        let project = settings.projects.iter().find(|project| {
            fs::canonicalize(&project.path)
                .ok()
                .is_some_and(|existing| existing == path)
        });
        if project.is_some_and(|project| {
            project.discovery == Discovery::Explicit && project.roots.is_empty()
        }) {
            return Err(Error::Usage(
                "explicit project discovery requires at least one --project-root".into(),
            ));
        }
    }
    Ok(())
}

fn build_plan(
    config_path: &Path,
    request: &InitRequest,
    settings: Config,
    agents: Vec<Agent>,
    previous: Journal,
) -> Result<SetupPlan, Error> {
    let reserved = [
        absolute_destination(config_path)?,
        absolute_destination(&journal_path())?,
        absolute_destination(&pending_path())?,
    ];
    if reserved.iter().collect::<HashSet<_>>().len() != reserved.len() {
        return Err(Error::Usage(
            "configuration and setup journal destinations must be distinct".into(),
        ));
    }
    let mut desired = BTreeMap::<PathBuf, Vec<u8>>::new();
    let mut updated_targets = previous.targets.clone();
    let mut reference_owners = HashSet::<(PathBuf, String)>::new();
    let mut context_owners = HashSet::<PathBuf>::new();
    for target in &previous.targets {
        if target.reference_added {
            reference_owners.insert((target.instructions_file.clone(), target.reference.clone()));
        }
        if target.context_created {
            context_owners.insert(target.context_file.clone());
        }
    }
    let mut summary = Vec::new();
    summary.push(format!("config: {}", config_path.display()));
    append_source_summary(&mut summary, &settings, request);

    for agent in agents {
        if agent == Agent::None {
            continue;
        }
        let paths = target_paths(agent, request, &settings)?;
        if paths.context_file == paths.instructions_file
            || reserved.contains(&paths.context_file)
            || reserved.contains(&paths.instructions_file)
        {
            return Err(Error::Usage(
                "configuration, journal, context and instructions destinations must not overlap"
                    .into(),
            ));
        }
        summary.push(format!("agent: {}", display_agent(agent)));
        summary.push(format!("context: {}", paths.context_file.display()));
        summary.push(format!(
            "instructions: {}",
            paths.instructions_file.display()
        ));

        let previous_target = previous
            .targets
            .iter()
            .find(|target| same_target(target, &paths));
        let current_context = planned_or_read(&desired, &paths.context_file)?;
        let owned_context = previous_target
            .filter(|target| target.context_created)
            .or_else(|| {
                previous.targets.iter().find(|target| {
                    target.context_file == paths.context_file
                        && target.context_created
                        && current_context
                            .as_ref()
                            .is_some_and(|contents| hash(contents) == target.context_hash)
                })
            });
        let context_created = previous_target
            .map(|target| target.context_created)
            .unwrap_or_else(|| {
                !context_owners.contains(&paths.context_file) && current_context.is_none()
            });
        if let Some(contents) = &current_context {
            if contents != CONTEXT.as_bytes()
                && !owned_context.is_some_and(|target| {
                    hash(contents) == target.context_hash && target.context_created
                })
            {
                return Err(Error::Operational(format!(
                    "unmanaged Skillwick context collision: {}",
                    paths.context_file.display()
                )));
            }
        }
        if current_context.as_deref() != Some(CONTEXT.as_bytes()) {
            desired.insert(paths.context_file.clone(), CONTEXT.as_bytes().to_vec());
        }

        let current_instructions = planned_or_read(&desired, &paths.instructions_file)?;
        let current_text = current_instructions
            .as_deref()
            .map(|bytes| {
                std::str::from_utf8(bytes).map(str::to_owned).map_err(|_| {
                    Error::Operational(format!(
                        "agent instructions are not UTF-8: {}",
                        paths.instructions_file.display()
                    ))
                })
            })
            .transpose()?
            .unwrap_or_default();
        let count = reference_count(&current_text, &paths.reference);
        if count > 1 {
            return Err(Error::Operational(format!(
                "ambiguous duplicate Skillwick context references: {}",
                paths.instructions_file.display()
            )));
        }
        let reference_key = (paths.instructions_file.clone(), paths.reference.clone());
        let reference_added = if count == 0 {
            // A previously owned reference remains owned when setup repairs a
            // user removal. Otherwise the repair would be indistinguishable
            // from a borrowed pre-existing line and uninstall would leak it.
            let added = previous_target.is_some_and(|target| target.reference_added)
                || !reference_owners.contains(&reference_key);
            let updated = add_reference(&current_text, &paths.reference)?;
            desired.insert(paths.instructions_file.clone(), updated.into_bytes());
            if added {
                reference_owners.insert(reference_key);
            }
            added
        } else {
            previous_target.is_some_and(|target| target.reference_added)
        };
        let instructions_created = previous_target
            .map(|target| target.instructions_created)
            .unwrap_or_else(|| current_instructions.is_none() && reference_added);
        let target_identity = paths.clone();
        let record = JournalTarget {
            agent: paths.agent,
            project: paths.project.clone(),
            instructions_file: paths.instructions_file.clone(),
            context_file: paths.context_file.clone(),
            reference: paths.reference.clone(),
            reference_added,
            instructions_created,
            context_hash: hash(CONTEXT.as_bytes()),
            context_created,
        };
        if let Some(position) = updated_targets
            .iter()
            .position(|target| same_target(target, &target_identity))
        {
            updated_targets[position] = record;
        } else {
            updated_targets.push(record);
        }
    }

    let config_bytes = toml_edit::ser::to_string_pretty(&settings)
        .map_err(|error| Error::Operational(error.to_string()))?
        .into_bytes();
    desired.insert(config_path.to_path_buf(), config_bytes);
    let journal = if updated_targets.is_empty() {
        None
    } else {
        updated_targets.sort_by_key(journal_key);
        Some(Journal {
            version: JOURNAL_VERSION,
            targets: updated_targets,
        })
    };
    if let Some(journal) = &journal {
        let bytes = serde_json::to_vec_pretty(journal)
            .map_err(|error| Error::Operational(error.to_string()))?;
        desired.insert(journal_path(), bytes);
    }
    let changes = changes_for(&desired)?;
    Ok(SetupPlan {
        settings,
        changes,
        summary,
    })
}

fn target_paths(
    agent: Agent,
    request: &InitRequest,
    settings: &Config,
) -> Result<TargetPaths, Error> {
    let project = if request.project {
        Some(config::normalize_cwd(&request.cwd).map_err(Error::Usage)?)
    } else {
        None
    };
    let (host_home, default_name) = match agent {
        Agent::Codex => (config::codex_home(), "AGENTS.md"),
        Agent::Claude => (config::claude_home(), "CLAUDE.md"),
        Agent::None => unreachable!("None has no integration target"),
    };
    let context_file = project
        .as_ref()
        .map(|path| path.join("SKILLWICK.md"))
        .unwrap_or_else(|| host_home.join("SKILLWICK.md"));
    let instructions_file = request
        .instructions_file
        .clone()
        .or_else(|| settings.instructions_file.clone())
        .unwrap_or_else(|| {
            project
                .as_ref()
                .map(|path| path.join(default_name))
                .unwrap_or_else(|| host_home.join(default_name))
        });
    let context_file = absolute_destination(&context_file)?;
    let instructions_file = absolute_destination(&instructions_file)?;
    let reference = reference_line(&context_file)?;
    config::refuse_symlink(&context_file).map_err(Error::Operational)?;
    config::refuse_symlink(&instructions_file).map_err(Error::Operational)?;
    Ok(TargetPaths {
        agent,
        project,
        instructions_file,
        context_file,
        reference,
    })
}

fn append_source_summary(summary: &mut Vec<String>, settings: &Config, request: &InitRequest) {
    summary.push(format!(
        "discovery: {}",
        display_discovery(settings.discovery)
    ));
    if settings.discovery == Discovery::Explicit {
        for root in &settings.roots {
            summary.push(format!("source: global {}", root.display()));
        }
    } else {
        for root in auto_global_roots() {
            summary.push(format!("source: global auto {}", root.display()));
        }
    }
    if request.project {
        if let Some(project) = settings.projects.iter().find(|project| {
            fs::canonicalize(&project.path)
                .ok()
                .zip(config::normalize_cwd(&request.cwd).ok())
                .is_some_and(|(left, right)| left == right)
        }) {
            summary.push(format!(
                "project: {} ({})",
                project.path.display(),
                display_discovery(project.discovery)
            ));
            if project.discovery == Discovery::Explicit {
                for root in &project.roots {
                    summary.push(format!("source: project {}", root.display()));
                }
            } else if let Ok(path) = config::normalize_cwd(&request.cwd) {
                for root in auto_project_roots(&path) {
                    summary.push(format!("source: project auto {}", root.display()));
                }
            }
        }
    }
}

fn auto_global_roots() -> Vec<PathBuf> {
    dedup_paths([
        config::home().join(".agents/skills"),
        config::codex_home().join("skills"),
        config::claude_home().join("skills"),
    ])
}

fn auto_project_roots(cwd: &Path) -> Vec<PathBuf> {
    dedup_paths([
        cwd.join(".agents/skills"),
        cwd.join(".codex/skills"),
        cwd.join(".claude/skills"),
    ])
}

fn dedup_paths<const N: usize>(paths: [PathBuf; N]) -> Vec<PathBuf> {
    let mut result = Vec::new();
    for path in paths {
        if !result.contains(&path) {
            result.push(path);
        }
    }
    result
}

fn apply_plan(plan: &SetupPlan) -> Result<Config, Error> {
    apply_changes(plan.changes.clone())?;
    Ok(plan.settings.clone())
}

fn changes_for(desired: &BTreeMap<PathBuf, Vec<u8>>) -> Result<Vec<Change>, Error> {
    let mut changes = Vec::new();
    for (path, after) in desired {
        let before = read_optional(path)?;
        if before.as_deref() != Some(after.as_slice()) {
            changes.push(Change {
                path: absolute_destination(path)?,
                before,
                after: Some(after.clone()),
            });
        }
    }
    Ok(changes)
}

pub fn uninstall(purge_cache: bool) -> Result<(), Error> {
    let _lock = acquire_lock()?;
    recover_pending()?;
    let Some(journal) = read_journal()? else {
        if purge_cache {
            let mut desired = BTreeMap::new();
            desired.insert(config::cache_path(), None);
            apply_changes(changes_for_optional(&desired)?)?;
        }
        return Ok(());
    };
    let mut desired = BTreeMap::<PathBuf, Option<Vec<u8>>>::new();
    let mut drift = Vec::new();
    for target in &journal.targets {
        let current = planned_or_read_optional(&desired, &target.instructions_file)?;
        if target.reference_added {
            let Some(contents) = current else {
                continue;
            };
            let text = match std::str::from_utf8(&contents) {
                Ok(text) => text,
                Err(_) => {
                    drift.push(format!(
                        "agent instructions are not UTF-8: {}",
                        target.instructions_file.display()
                    ));
                    continue;
                }
            };
            match reference_count(text, &target.reference) {
                0 => {}
                1 => match remove_reference(text, &target.reference) {
                    Ok(updated) => {
                        let after = if target.instructions_created && updated.is_empty() {
                            None
                        } else {
                            Some(updated.into_bytes())
                        };
                        desired.insert(target.instructions_file.clone(), after);
                    }
                    Err(error) => drift.push(error.to_string()),
                },
                _ => drift.push(format!(
                    "ambiguous duplicate Skillwick context references: {}",
                    target.instructions_file.display()
                )),
            }
        }
    }
    for target in &journal.targets {
        if !target.context_created {
            continue;
        }
        let current = planned_or_read_optional(&desired, &target.context_file)?;
        let Some(contents) = current else {
            continue;
        };
        if config::refuse_symlink(&target.context_file).is_err()
            || hash(&contents) != target.context_hash
        {
            drift.push(format!(
                "Skillwick context changed after setup: {}",
                target.context_file.display()
            ));
        } else {
            desired.insert(target.context_file.clone(), None);
        }
    }
    if !drift.is_empty() {
        return Err(Error::Operational(format!(
            "integration drift: {}",
            drift.join("; ")
        )));
    }
    desired.insert(journal_path(), None);
    if purge_cache {
        desired.insert(config::cache_path(), None);
    }
    apply_changes(changes_for_optional(&desired)?)
}

fn apply_changes(changes: Vec<Change>) -> Result<(), Error> {
    let transaction = Transaction {
        version: TRANSACTION_VERSION,
        committed: false,
        changes: changes.clone(),
    };
    let pending = pending_path();
    let bytes = serde_json::to_vec_pretty(&transaction)
        .map_err(|error| Error::Operational(error.to_string()))?;
    config::atomic_write(&pending, &bytes, 0o600).map_err(Error::Operational)?;
    for change in &changes {
        let current = read_optional(&change.path)?;
        if current != change.before {
            return Err(Error::Operational(format!(
                "setup destination changed while waiting: {}",
                change.path.display()
            )));
        }
        match &change.after {
            Some(contents) => {
                config::atomic_write(&change.path, contents, 0o600).map_err(Error::Operational)?
            }
            None => remove_owned_file(&change.path)?,
        }
    }
    let committed = Transaction {
        version: TRANSACTION_VERSION,
        committed: true,
        changes,
    };
    let committed_bytes = serde_json::to_vec_pretty(&committed)
        .map_err(|error| Error::Operational(error.to_string()))?;
    config::atomic_write(&pending, &committed_bytes, 0o600).map_err(Error::Operational)?;
    fs::remove_file(&pending)
        .map_err(|error| Error::Operational(format!("{}: {error}", pending.display())))?;
    Ok(())
}

fn changes_for_optional(
    desired: &BTreeMap<PathBuf, Option<Vec<u8>>>,
) -> Result<Vec<Change>, Error> {
    let mut changes = Vec::new();
    for (path, after) in desired {
        let before = read_optional(path)?;
        if before != *after {
            changes.push(Change {
                path: absolute_destination(path)?,
                before,
                after: after.clone(),
            });
        }
    }
    Ok(changes)
}

/// Check the owned integration receipt and every target it records.
pub fn diagnostics(expected_agents: &[Agent]) -> Result<Vec<String>, Error> {
    let mut diagnostics = Vec::new();
    let pending = pending_path();
    if fs::symlink_metadata(&pending).is_ok() {
        diagnostics.push(format!(
            "pending Skillwick setup transaction requires recovery: {}",
            pending.display()
        ));
    }
    let journal = read_journal()?;
    let Some(journal) = journal else {
        for agent in expected_agents
            .iter()
            .copied()
            .filter(|agent| *agent != Agent::None)
        {
            diagnostics.push(format!(
                "Skillwick integration journal is missing a {} target",
                display_agent(agent)
            ));
        }
        return Ok(diagnostics);
    };
    for agent in expected_agents
        .iter()
        .copied()
        .filter(|agent| *agent != Agent::None)
    {
        if !journal.targets.iter().any(|target| target.agent == agent) {
            diagnostics.push(format!(
                "Skillwick integration journal is missing a {} target",
                display_agent(agent)
            ));
        }
    }
    for target in &journal.targets {
        if config::refuse_symlink(&target.context_file).is_err() {
            diagnostics.push(format!(
                "Skillwick context destination is symlinked: {}",
                target.context_file.display()
            ));
        } else {
            match read_optional(&target.context_file) {
                Ok(Some(contents)) if hash(&contents) != target.context_hash => {
                    diagnostics.push(format!(
                        "Skillwick context changed after setup: {}",
                        target.context_file.display()
                    ));
                }
                Ok(None) => diagnostics.push(format!(
                    "Skillwick context is missing: {}",
                    target.context_file.display()
                )),
                Err(error) => diagnostics.push(error.to_string()),
                _ => {}
            }
        }
        if config::refuse_symlink(&target.instructions_file).is_err() {
            diagnostics.push(format!(
                "agent instructions destination is symlinked: {}",
                target.instructions_file.display()
            ));
        } else {
            match fs::read_to_string(&target.instructions_file) {
                Ok(text) if reference_count(&text, &target.reference) == 1 => {}
                Ok(_) => diagnostics.push(format!(
                    "Skillwick context reference changed after setup: {}",
                    target.instructions_file.display()
                )),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    diagnostics.push(format!(
                        "agent instructions are missing: {}",
                        target.instructions_file.display()
                    ))
                }
                Err(error) => {
                    diagnostics.push(format!("{}: {error}", target.instructions_file.display()))
                }
            }
        }
    }
    Ok(diagnostics)
}

fn read_journal() -> Result<Option<Journal>, Error> {
    let path = journal_path();
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Error::Operational(format!("{}: {error}", path.display()))),
    };
    let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
        Error::Operational(format!(
            "invalid Skillwick integration journal at {}: {error}; back it up and re-run `skillwick init`",
            path.display()
        ))
    })?;
    let version = value.get("version").and_then(Value::as_u64);
    if version != Some(u64::from(JOURNAL_VERSION)) {
        return Err(Error::Operational(format!(
            "obsolete Skillwick integration journal at {}; back it up and re-run `skillwick init`",
            path.display()
        )));
    }
    let journal: Journal = serde_json::from_value(value).map_err(|error| {
        Error::Operational(format!(
            "invalid Skillwick integration journal at {}: {error}; back it up and re-run `skillwick init`",
            path.display()
        ))
    })?;
    if journal
        .targets
        .iter()
        .any(|target| !target.instructions_file.is_absolute() || !target.context_file.is_absolute())
    {
        return Err(Error::Operational(
            "integration journal contains relative destinations; preserve it for manual recovery"
                .into(),
        ));
    }
    Ok(Some(journal))
}

fn recover_pending() -> Result<(), Error> {
    let path = pending_path();
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(Error::Operational(format!("{}: {error}", path.display()))),
    };
    let transaction: Transaction = serde_json::from_slice(&bytes).map_err(|error| {
        Error::Operational(format!(
            "invalid pending Skillwick setup journal at {}: {error}; back it up and re-run `skillwick init`",
            path.display()
        ))
    })?;
    if transaction.version != TRANSACTION_VERSION {
        return Err(Error::Operational(format!(
            "obsolete pending Skillwick setup journal at {}; back it up and re-run `skillwick init`",
            path.display()
        )));
    }
    if transaction
        .changes
        .iter()
        .any(|change| !change.path.is_absolute())
    {
        return Err(Error::Operational(
            "pending setup journal contains relative destinations; preserve it for manual recovery"
                .into(),
        ));
    }
    if transaction.committed {
        fs::remove_file(&path)
            .map_err(|error| Error::Operational(format!("{}: {error}", path.display())))?;
        return Ok(());
    }
    for change in &transaction.changes {
        let current = read_optional(&change.path)?;
        if current == change.before {
            continue;
        }
        if current != change.after {
            return Err(Error::Operational(format!(
                "pending setup destination changed after interruption: {}; preserve it and re-run setup",
                change.path.display()
            )));
        }
        match &change.before {
            Some(contents) => {
                config::atomic_write(&change.path, contents, 0o600).map_err(Error::Operational)?
            }
            None => remove_owned_file(&change.path)?,
        }
    }
    fs::remove_file(&path)
        .map_err(|error| Error::Operational(format!("{}: {error}", path.display())))?;
    Ok(())
}

fn acquire_lock() -> Result<SetupLock, Error> {
    let directory = config::state_dir();
    fs::create_dir_all(&directory)
        .map_err(|error| Error::Operational(format!("{}: {error}", directory.display())))?;
    let path = directory.join("setup.lock");
    for attempt in 0..2 {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(mut file) => {
                file.write_all(std::process::id().to_string().as_bytes())
                    .map_err(|error| Error::Operational(format!("{}: {error}", path.display())))?;
                file.sync_all()
                    .map_err(|error| Error::Operational(format!("{}: {error}", path.display())))?;
                return Ok(SetupLock { path });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && attempt == 0 => {
                if stale_lock(&path) {
                    fs::remove_file(&path).map_err(|remove_error| {
                        Error::Operational(format!("{}: {remove_error}", path.display()))
                    })?;
                    continue;
                }
                return Err(Error::Operational(format!(
                    "another Skillwick setup is in progress: {}",
                    path.display()
                )));
            }
            Err(error) => {
                return Err(Error::Operational(format!("{}: {error}", path.display())));
            }
        }
    }
    unreachable!("lock acquisition either succeeds or returns")
}

fn stale_lock(path: &Path) -> bool {
    let Ok(contents) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(pid) = contents.trim().parse::<u32>() else {
        return false;
    };
    if pid == std::process::id() {
        return false;
    }
    #[cfg(unix)]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .is_ok_and(|status| !status.success())
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

fn journal_path() -> PathBuf {
    config::state_dir().join("integration.json")
}

fn pending_path() -> PathBuf {
    config::state_dir().join("integration.pending.json")
}

fn absolute_destination(path: &Path) -> Result<PathBuf, Error> {
    let absolute = std::path::absolute(path).map_err(|error| {
        Error::Operational(format!(
            "cannot resolve destination {}: {error}",
            path.display()
        ))
    })?;
    config::refuse_symlink(&absolute).map_err(Error::Operational)?;
    // Resolve the existing ancestor; missing destination directories may be
    // created later. This also gives alias paths one ownership identity.
    let mut ancestor = absolute.as_path();
    let mut tail = Vec::new();
    loop {
        match fs::canonicalize(ancestor) {
            Ok(mut resolved) => {
                for component in tail.iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let name = ancestor.file_name().ok_or_else(|| {
                    Error::Operational(format!("cannot resolve destination {}", absolute.display()))
                })?;
                tail.push(name.to_os_string());
                ancestor = ancestor.parent().ok_or_else(|| {
                    Error::Operational(format!("cannot resolve destination {}", absolute.display()))
                })?;
            }
            Err(error) => {
                return Err(Error::Operational(format!(
                    "cannot resolve destination {}: {error}",
                    absolute.display()
                )))
            }
        }
    }
}

fn normalize_root(root: &Path) -> Result<PathBuf, Error> {
    let root = fs::canonicalize(root).map_err(|error| {
        Error::Usage(format!(
            "cannot normalize skill root {}: {error}",
            root.display()
        ))
    })?;
    if root.is_dir() {
        Ok(root)
    } else {
        Err(Error::Usage(format!(
            "skill root is not a directory: {}",
            root.display()
        )))
    }
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, Error> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::Operational(format!("{}: {error}", path.display()))),
    }
}

fn planned_or_read(
    desired: &BTreeMap<PathBuf, Vec<u8>>,
    path: &Path,
) -> Result<Option<Vec<u8>>, Error> {
    desired
        .get(path)
        .cloned()
        .map(Some)
        .map_or_else(|| read_optional(path), Ok)
}

fn planned_or_read_optional(
    desired: &BTreeMap<PathBuf, Option<Vec<u8>>>,
    path: &Path,
) -> Result<Option<Vec<u8>>, Error> {
    desired
        .get(path)
        .cloned()
        .map_or_else(|| read_optional(path), Ok)
}

fn remove_owned_file(path: &Path) -> Result<(), Error> {
    if !path.exists() {
        return Ok(());
    }
    config::refuse_symlink(path).map_err(Error::Operational)?;
    fs::remove_file(path)
        .map_err(|error| Error::Operational(format!("{}: {error}", path.display())))
}

fn reference_line(context: &Path) -> Result<String, Error> {
    let context = context
        .to_str()
        .ok_or_else(|| Error::Usage("Skillwick context path is not UTF-8".into()))?;
    if context.contains(['\r', '\n']) {
        return Err(Error::Usage(
            "Skillwick context path contains a newline".into(),
        ));
    }
    Ok(format!("@{context}"))
}

fn reference_count(current: &str, reference: &str) -> usize {
    current
        .split_inclusive('\n')
        .filter(|segment| {
            let line = segment.strip_suffix('\n').unwrap_or(segment);
            line.strip_suffix('\r').unwrap_or(line) == reference
        })
        .count()
}

fn add_reference(current: &str, reference: &str) -> Result<String, Error> {
    let count = reference_count(current, reference);
    if count > 1 {
        return Err(Error::Operational(
            "ambiguous duplicate Skillwick context references".into(),
        ));
    }
    if count == 1 {
        return Ok(current.to_owned());
    }
    let newline = if current.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    Ok(if current.is_empty() {
        format!("{reference}{newline}")
    } else if current.ends_with(newline) {
        format!("{current}{reference}{newline}")
    } else {
        format!("{current}{newline}{reference}{newline}")
    })
}

fn remove_reference(current: &str, reference: &str) -> Result<String, Error> {
    if reference_count(current, reference) != 1 {
        return Err(Error::Operational(
            "Skillwick context reference changed after setup".into(),
        ));
    }
    Ok(current
        .split_inclusive('\n')
        .filter(|segment| {
            let line = segment.strip_suffix('\n').unwrap_or(segment);
            line.strip_suffix('\r').unwrap_or(line) != reference
        })
        .collect())
}

fn same_target(target: &JournalTarget, paths: &TargetPaths) -> bool {
    target.agent == paths.agent
        && target.project == paths.project
        && target.instructions_file == paths.instructions_file
        && target.context_file == paths.context_file
}

fn journal_key(target: &JournalTarget) -> String {
    format!(
        "{}:{}:{}",
        agent_key(target.agent),
        target
            .project
            .as_deref()
            .unwrap_or_else(|| Path::new(""))
            .display(),
        target.instructions_file.display()
    )
}

fn agent_key(agent: Agent) -> u8 {
    match agent {
        Agent::Codex => 0,
        Agent::Claude => 1,
        Agent::None => 2,
    }
}

fn display_agent(agent: Agent) -> &'static str {
    match agent {
        Agent::Codex => "codex",
        Agent::Claude => "claude",
        Agent::None => "none",
    }
}

fn display_discovery(discovery: Discovery) -> &'static str {
    match discovery {
        Discovery::Auto => "auto",
        Discovery::Explicit => "explicit",
    }
}

fn print_plan(plan: &SetupPlan) -> Result<(), Error> {
    let stderr = io::stderr();
    let mut output = stderr.lock();
    for line in &plan.summary {
        match writeln!(output, "{line}") {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => return Ok(()),
            Err(error) => return Err(Error::Operational(error.to_string())),
        }
    }
    Ok(())
}

fn report_pending(message: &str) -> Result<(), Error> {
    let stderr = io::stderr();
    let mut output = stderr.lock();
    match writeln!(output, "{message}") {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(Error::Operational(error.to_string())),
    }
}

fn confirm() -> Result<(), Error> {
    if !interactive() {
        return Err(Error::Usage("non-interactive init requires --yes".into()));
    }
    if inquire::Confirm::new("Apply this plan?")
        .with_default(false)
        .prompt()
        .map_err(|error| Error::Operational(error.to_string()))?
    {
        Ok(())
    } else {
        Err(Error::Usage("setup cancelled".into()))
    }
}

fn interactive() -> bool {
    io::stdin().is_terminal() && io::stderr().is_terminal()
}

fn hash(contents: &[u8]) -> String {
    format!("{:x}", Sha256::digest(contents))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_are_idempotent_and_preserve_crlf() {
        let reference = "@/tmp/codex/SKILLWICK.md";
        let once = add_reference("before\r\n", reference).unwrap();
        let twice = add_reference(&once, reference).unwrap();
        assert_eq!(once, twice);
        assert_eq!(reference_count(&twice, reference), 1);
        assert!(twice.contains("\r\n"));
        assert_eq!(remove_reference(&twice, reference).unwrap(), "before\r\n");
    }

    #[test]
    fn remove_reference_removes_one_owned_line() {
        let source = "before\n@/tmp/SKILLWICK.md\nafter\n";
        assert_eq!(reference_count(source, "@/tmp/SKILLWICK.md"), 1);
        assert_eq!(
            remove_reference(source, "@/tmp/SKILLWICK.md").unwrap(),
            "before\nafter\n"
        );
    }
}
