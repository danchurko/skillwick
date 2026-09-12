use crate::{
    config::{self, Agent, Config, Hooks, Inventory},
    index, native, sources,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::IsTerminal,
    path::{Path, PathBuf},
};
use toml_edit::{value, DocumentMut};

const BEGIN: &str = "<!-- skillwick:begin -->";
const END: &str = "<!-- skillwick:end -->";
const CONTEXT: &str = include_str!("../assets/skillwick/SKILLWICK.md");
const HOOK_STATUS: &str = "Finding relevant skills with Skillwick";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Catalog {
    Auto,
    Native,
    Unchanged,
}

pub struct InitRequest {
    pub cwd: PathBuf,
    pub yes: bool,
    pub dry_run: bool,
    pub agent: Agent,
    pub inventory: Inventory,
    pub catalog: Catalog,
    pub hooks: Hooks,
    pub roots: Vec<PathBuf>,
    pub codex_home: Option<PathBuf>,
    pub codex_bin: Option<PathBuf>,
    pub instructions_file: Option<PathBuf>,
}

#[derive(Deserialize, Serialize)]
struct Journal {
    version: u8,
    instructions_file: PathBuf,
    #[serde(default)]
    context_file: Option<PathBuf>,
    #[serde(default)]
    context_hash: Option<String>,
    #[serde(default)]
    context_file_created: bool,
    #[serde(default)]
    reference: Option<String>,
    #[serde(default)]
    reference_added: bool,
    #[serde(default)]
    legacy_block: bool,
    #[serde(default)]
    router_file: Option<PathBuf>,
    codex_config: PathBuf,
    previous_catalog: Option<bool>,
    wrote_catalog: bool,
    #[serde(default)]
    router_hash: Option<String>,
    #[serde(default)]
    hook_file: Option<PathBuf>,
    #[serde(default)]
    hook_command: Option<String>,
    #[serde(default)]
    hook_file_created: bool,
}

pub fn init(config_path: &Path, request: InitRequest) -> Result<Config, String> {
    let mut settings = config::load(config_path)?;
    for root in request.roots {
        if !settings.roots.contains(&root) {
            settings.roots.push(root);
        }
    }
    settings.agent = request.agent;
    settings.inventory = request.inventory;
    settings.hooks = request.hooks;
    if request.codex_home.is_some() {
        settings.codex_home = request.codex_home;
    }
    if request.codex_bin.is_some() {
        settings.codex_bin = request.codex_bin;
    }
    if request.instructions_file.is_some() {
        settings.instructions_file = request.instructions_file;
    }
    if settings.agent == Agent::None && settings.hooks == Hooks::Suggest {
        return Err("--hooks suggest requires --agent codex".into());
    }
    if settings.agent == Agent::None {
        print_plan(config_path, &settings, false);
        if request.dry_run {
            return Ok(settings);
        }
        confirm(request.yes)?;
        config::save(config_path, &settings)?;
        return Ok(settings);
    }
    let codex_home = config::codex_home(&settings);
    let instructions = effective_instructions(&settings, &codex_home)?;
    let codex_config = codex_home.join("config.toml");
    let hook_file = codex_home.join("hooks.json");
    let previous_journal = fs::read(config::state_dir().join("integration.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Journal>(&bytes).ok());
    let context = context_file(&codex_home)?;
    let reference = reference_line(&context)?;
    let codex = native::detect(&settings)?;
    let compatible = native::supports_native_catalog(&codex.version);
    if settings.hooks == Hooks::Suggest && !compatible {
        return Err(format!(
            "Codex {} does not have a verified suggestion-hook contract",
            codex.version
        ));
    }
    let write_catalog = match request.catalog {
        Catalog::Native if !compatible => {
            return Err(format!(
                "Codex {} does not have a verified native catalogue contract",
                codex.version
            ))
        }
        Catalog::Native => true,
        Catalog::Auto => compatible,
        Catalog::Unchanged => false,
    };
    if !compatible && request.catalog == Catalog::Auto {
        settings.inventory = Inventory::Filesystem;
        eprintln!(
            "warning: Codex {} is unverified; using discovery-only filesystem inventory",
            codex.version
        );
    }
    print_plan(config_path, &settings, write_catalog);
    if request.dry_run {
        return Ok(settings);
    }
    confirm(request.yes)?;
    config::refuse_symlink(&instructions)?;
    config::refuse_symlink(&codex_config)?;
    config::refuse_symlink(&context)?;
    if settings.hooks == Hooks::Suggest
        || previous_journal
            .as_ref()
            .is_some_and(|journal| journal.hook_command.is_some())
    {
        config::refuse_symlink(&hook_file)?;
    }
    let context_previous = fs::read(&context).ok();
    let context_owned = previous_journal.as_ref().is_some_and(|journal| {
        journal.context_file_created
            && journal.context_file.as_ref() == Some(&context)
            && journal.context_hash.as_ref().is_some_and(|expected| {
                context_previous
                    .as_ref()
                    .is_some_and(|contents| hash(contents) == *expected)
            })
    });
    if context_previous
        .as_deref()
        .is_some_and(|contents| contents != CONTEXT.as_bytes())
        && !context_owned
    {
        return Err(format!(
            "unmanaged Skillwick context collision: {}",
            context.display()
        ));
    }
    let legacy_block = previous_journal
        .as_ref()
        .is_some_and(|journal| journal.version == 1 || journal.legacy_block);
    let legacy_router = if let Some(journal) = previous_journal.as_ref() {
        if let (Some(router), Some(router_hash)) = (&journal.router_file, &journal.router_hash) {
            if router.exists() {
                config::refuse_symlink(router)?;
                if !fs::read(router)
                    .ok()
                    .is_some_and(|contents| hash(&contents) == *router_hash)
                {
                    return Err(format!("router changed after setup: {}", router.display()));
                }
                Some(router.clone())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };
    let mut instruction_previous = fs::read_to_string(&instructions).unwrap_or_default();
    if legacy_block {
        let begins = instruction_previous.matches(BEGIN).count();
        let ends = instruction_previous.matches(END).count();
        if begins == 1 && ends == 1 {
            instruction_previous =
                remove_block(&instruction_previous).map_err(|error| error.to_string())?;
        } else if begins != 0
            || ends != 0
            || reference_count(&instruction_previous, &reference) != 1
        {
            return Err("legacy Skillwick instruction block changed after setup".into());
        }
    }
    let instruction_reference_count = reference_count(&instruction_previous, &reference);
    if instruction_reference_count > 1 {
        return Err("ambiguous duplicate Skillwick context references".into());
    }
    let instruction_updated = add_reference(&instruction_previous, &reference)?;
    let continuing_context = previous_journal.as_ref().is_some_and(|journal| {
        journal.instructions_file == instructions
            && journal.context_file.as_ref() == Some(&context)
            && journal.reference.as_deref() == Some(reference.as_str())
    });
    let context_file_created = if continuing_context {
        previous_journal
            .as_ref()
            .is_some_and(|journal| journal.context_file_created)
    } else {
        !context.exists()
    };
    let reference_added = if continuing_context {
        previous_journal
            .as_ref()
            .is_some_and(|journal| journal.reference_added)
    } else {
        instruction_reference_count == 0
    };
    let (observed_catalog, codex_updated) = if write_catalog {
        patch_catalog(
            &fs::read_to_string(&codex_config).unwrap_or_default(),
            false,
        )?
    } else {
        (None, None)
    };
    let continuing_catalog = previous_journal
        .as_ref()
        .is_some_and(|journal| journal.wrote_catalog && journal.codex_config == codex_config);
    let catalog_previous = if continuing_catalog {
        previous_journal
            .as_ref()
            .and_then(|journal| journal.previous_catalog)
    } else {
        observed_catalog
    };
    let hook_file_created = previous_journal
        .as_ref()
        .is_some_and(|journal| journal.hook_file_created)
        || !hook_file.exists();
    let mut hook_updated = fs::read_to_string(&hook_file).unwrap_or_default();
    if let Some(command) = previous_journal
        .as_ref()
        .and_then(|journal| journal.hook_command.as_deref())
    {
        if settings.hooks == Hooks::Off
            || command != hook_command(&std::env::current_exe().map_err(|e| e.to_string())?)?
        {
            hook_updated = remove_hook(&hook_updated, command)?;
        }
    }
    let hook_command = if settings.hooks == Hooks::Suggest {
        let command = hook_command(&std::env::current_exe().map_err(|e| e.to_string())?)?;
        hook_updated = add_hook(&hook_updated, &command)?;
        Some(command)
    } else {
        None
    };
    let journal = Journal {
        version: 2,
        instructions_file: instructions.clone(),
        context_file: Some(context.clone()),
        context_hash: Some(hash(CONTEXT.as_bytes())),
        context_file_created,
        reference: Some(reference),
        reference_added,
        legacy_block,
        router_file: previous_journal
            .as_ref()
            .and_then(|journal| journal.router_file.clone()),
        codex_config: codex_config.clone(),
        previous_catalog: catalog_previous,
        wrote_catalog: write_catalog || continuing_catalog,
        router_hash: previous_journal
            .as_ref()
            .and_then(|journal| journal.router_hash.clone()),
        hook_file: hook_command.as_ref().map(|_| hook_file.clone()),
        hook_command,
        hook_file_created,
    };
    prime_index(&settings, &codex, &request.cwd)
        .map_err(|error| format!("{error}; native catalogue was not changed"))?;
    let journal_path = config::state_dir().join("integration.json");
    config::atomic_write(
        &journal_path,
        &serde_json::to_vec(&journal).map_err(|e| e.to_string())?,
        0o600,
    )?;
    config::save(config_path, &settings)?;
    config::atomic_write(&context, CONTEXT.as_bytes(), 0o600)?;
    config::atomic_write(&instructions, instruction_updated.as_bytes(), 0o600)?;
    if settings.hooks == Hooks::Suggest
        || previous_journal
            .as_ref()
            .is_some_and(|journal| journal.hook_command.is_some())
    {
        if settings.hooks == Hooks::Off && hook_file_created && hook_document_empty(&hook_updated) {
            if hook_file.exists() {
                fs::remove_file(&hook_file).map_err(|e| e.to_string())?;
            }
        } else {
            config::atomic_write(&hook_file, hook_updated.as_bytes(), 0o600)?;
        }
    }
    if let Some(updated) = codex_updated {
        config::atomic_write(&codex_config, updated.as_bytes(), 0o600)?;
    }
    if let Some(router) = &legacy_router {
        fs::remove_file(router).map_err(|e| e.to_string())?;
    }
    Ok(settings)
}

fn prime_index(settings: &Config, codex: &native::Codex, cwd: &Path) -> Result<(), String> {
    let scan = sources::scan(cwd, &settings.roots);
    let mut db = index::open(Path::new(":memory:")).map_err(|error| error.to_string())?;
    index::refresh_kind(&mut db, "filesystem", &scan.skills, scan.complete)
        .map_err(|error| error.to_string())?;
    let mut has_specialist = scan
        .skills
        .iter()
        .any(|skill| skill.metadata.name != "skillwick");
    if settings.inventory == Inventory::Codex {
        let native_skills = native::inventory(codex, &config::codex_home(settings), cwd)?;
        has_specialist |= native_skills
            .iter()
            .any(|skill| skill.metadata.name != "skillwick");
        index::refresh_kind(&mut db, "codex", &native_skills, true)
            .map_err(|error| error.to_string())?;
        index::record_snapshot(
            &db,
            cwd,
            &codex.version,
            &codex.path,
            &config::codex_home(settings),
        )
        .map_err(|error| error.to_string())?;
    }
    if !has_specialist {
        return Err("no specialist skill source could be indexed".into());
    }
    index::publish(&db, &config::cache_path()).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn uninstall(purge_cache: bool) -> Result<(), String> {
    let journal_path = config::state_dir().join("integration.json");
    let journal: Journal = serde_json::from_slice(
        &fs::read(&journal_path).map_err(|e| format!("{}: {e}", journal_path.display()))?,
    )
    .map_err(|e| e.to_string())?;
    let mut drift: Vec<String> = Vec::new();
    if journal.wrote_catalog {
        let current = fs::read_to_string(&journal.codex_config).map_err(|e| e.to_string())?;
        match restore_catalog(&current, journal.previous_catalog)? {
            Some(updated) => {
                config::atomic_write(&journal.codex_config, updated.as_bytes(), 0o600)?
            }
            None => drift.push("Codex catalogue setting changed after setup".into()),
        }
    }
    if let (Some(hook_file), Some(command)) = (&journal.hook_file, &journal.hook_command) {
        let current = fs::read_to_string(hook_file).unwrap_or_default();
        match remove_hook(&current, command) {
            Ok(updated) if journal.hook_file_created && hook_document_empty(&updated) => {
                if hook_file.exists() {
                    fs::remove_file(hook_file).map_err(|e| e.to_string())?;
                }
            }
            Ok(updated) => config::atomic_write(hook_file, updated.as_bytes(), 0o600)?,
            Err(error) => drift.push(error),
        }
    }
    let current = fs::read_to_string(&journal.instructions_file).unwrap_or_default();
    if let Some(reference) = &journal.reference {
        let mut updated = if journal.legacy_block && reference_count(&current, reference) == 0 {
            current.clone()
        } else {
            match remove_reference(&current, reference, journal.reference_added) {
                Ok(updated) => updated,
                Err(error) => {
                    drift.push(error);
                    current.clone()
                }
            }
        };
        if journal.legacy_block {
            let begins = updated.matches(BEGIN).count();
            let ends = updated.matches(END).count();
            if begins == 1 && ends == 1 {
                updated = remove_block(&updated).unwrap();
            } else if begins != 0 || ends != 0 {
                drift.push("legacy Skillwick instruction block changed after setup".into());
            }
        }
        if updated != current {
            config::atomic_write(&journal.instructions_file, updated.as_bytes(), 0o600)?
        }
    } else {
        match remove_block(&current) {
            Ok(updated) => {
                config::atomic_write(&journal.instructions_file, updated.as_bytes(), 0o600)?
            }
            Err(error) => drift.push(error.into()),
        }
    }
    if let (Some(context), Some(context_hash)) = (&journal.context_file, &journal.context_hash) {
        if journal.context_file_created {
            if config::refuse_symlink(context).is_err()
                || !fs::read(context)
                    .ok()
                    .is_some_and(|contents| hash(&contents) == *context_hash)
            {
                drift.push("Skillwick context changed after setup".into());
            } else {
                fs::remove_file(context).map_err(|e| e.to_string())?;
            }
        }
    }
    if let (Some(router), Some(router_hash)) = (&journal.router_file, &journal.router_hash) {
        if !router.exists() {
            // A v1-to-v2 migration already removed this legacy router.
        } else if fs::read(router)
            .ok()
            .is_some_and(|contents| hash(&contents) == *router_hash)
        {
            fs::remove_file(router).map_err(|e| e.to_string())?;
        } else {
            drift.push("router changed after setup".into())
        }
    }
    if purge_cache {
        let cache = config::cache_path();
        if cache.exists() {
            fs::remove_file(cache).map_err(|e| e.to_string())?;
        }
    }
    if drift.is_empty() {
        let _ = fs::remove_file(journal_path);
        Ok(())
    } else {
        Err(format!("integration drift: {}", drift.join("; ")))
    }
}

pub fn context_file(codex_home: &Path) -> Result<PathBuf, String> {
    let codex_home = if codex_home.is_absolute() {
        codex_home.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("cannot resolve Codex home: {e}"))?
            .join(codex_home)
    };
    Ok(codex_home.join("SKILLWICK.md"))
}

fn reference_line(context: &Path) -> Result<String, String> {
    let context = context
        .to_str()
        .ok_or("Skillwick context path is not UTF-8")?;
    if context.contains(['\r', '\n']) {
        return Err("Skillwick context path contains a newline".into());
    }
    Ok(format!("@{context}"))
}

pub fn integration_present(instructions: &Path, codex_home: &Path) -> bool {
    let Ok(context) = context_file(codex_home) else {
        return false;
    };
    let Ok(reference) = reference_line(&context) else {
        return false;
    };
    fs::read_to_string(&context).is_ok_and(|text| {
        text.contains("Skillwick is a skill helper.")
            && text.contains("`skillwick --json list --all`")
            && text.contains("`skillwick read ID`")
    }) && fs::read_to_string(instructions).is_ok_and(|text| reference_count(&text, &reference) == 1)
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

fn add_reference(current: &str, reference: &str) -> Result<String, String> {
    let count = reference_count(current, reference);
    if count > 1 {
        return Err("ambiguous duplicate Skillwick context references".into());
    }
    if count == 1 {
        return Ok(current.to_string());
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

fn remove_reference(current: &str, reference: &str, owned: bool) -> Result<String, String> {
    let count = reference_count(current, reference);
    if !owned {
        return Ok(current.to_string());
    }
    if count != 1 {
        return Err("Skillwick context reference changed after setup".into());
    }
    Ok(current
        .split_inclusive('\n')
        .filter(|segment| {
            let line = segment.strip_suffix('\n').unwrap_or(segment);
            line.strip_suffix('\r').unwrap_or(line) != reference
        })
        .collect())
}

fn hook_command(executable: &Path) -> Result<String, String> {
    let executable = executable
        .to_str()
        .ok_or("Skillwick executable path is not UTF-8")?;
    Ok(format!("'{}' hook", executable.replace('\'', "'\\''")))
}

fn hook_group(command: &str) -> Value {
    json!({"hooks": [{
        "type": "command",
        "command": command,
        "timeout": 2,
        "statusMessage": HOOK_STATUS
    }]})
}

fn hook_document(current: &str) -> Result<Value, String> {
    if current.trim().is_empty() {
        return Ok(json!({"hooks": {}}));
    }
    let value: Value = serde_json::from_str(current)
        .map_err(|error| format!("invalid Codex hooks JSON: {error}"))?;
    if !value.is_object() {
        return Err("Codex hooks JSON must be an object".into());
    }
    Ok(value)
}

fn add_hook(current: &str, command: &str) -> Result<String, String> {
    let mut document = hook_document(current)?;
    let root = document.as_object_mut().unwrap();
    let hooks = root.entry("hooks").or_insert_with(|| json!({}));
    let hooks = hooks
        .as_object_mut()
        .ok_or("Codex hooks field must be an object")?;
    let event = hooks.entry("UserPromptSubmit").or_insert_with(|| json!([]));
    let event = event
        .as_array_mut()
        .ok_or("Codex UserPromptSubmit hooks must be an array")?;
    let group = hook_group(command);
    if !event.contains(&group) {
        event.push(group);
    }
    Ok(format!(
        "{}\n",
        serde_json::to_string_pretty(&document).unwrap()
    ))
}

fn remove_hook(current: &str, command: &str) -> Result<String, String> {
    let mut document = hook_document(current)?;
    let Some(hooks) = document.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Err("Skillwick hook changed after setup".into());
    };
    let Some(event) = hooks
        .get_mut("UserPromptSubmit")
        .and_then(Value::as_array_mut)
    else {
        return Err("Skillwick hook changed after setup".into());
    };
    let group = hook_group(command);
    let Some(position) = event.iter().position(|candidate| candidate == &group) else {
        return Err("Skillwick hook changed after setup".into());
    };
    event.remove(position);
    if event.is_empty() {
        hooks.remove("UserPromptSubmit");
    }
    Ok(format!(
        "{}\n",
        serde_json::to_string_pretty(&document).unwrap()
    ))
}

fn hook_document_empty(current: &str) -> bool {
    hook_document(current).is_ok_and(|document| {
        document.as_object().is_some_and(|root| {
            root.len() == 1
                && root
                    .get("hooks")
                    .and_then(Value::as_object)
                    .is_some_and(|hooks| hooks.is_empty())
        })
    })
}

fn effective_instructions(settings: &Config, codex_home: &Path) -> Result<PathBuf, String> {
    if let Some(path) = &settings.instructions_file {
        return Ok(path.clone());
    }
    if codex_home.join("AGENTS.override.md").exists() {
        return Err("AGENTS.override.md is active; pass --instructions-file explicitly".into());
    }
    Ok(codex_home.join("AGENTS.md"))
}
fn remove_block(current: &str) -> Result<String, &'static str> {
    if current.matches(BEGIN).count() != 1 || current.matches(END).count() != 1 {
        return Err("instruction block changed after setup");
    }
    let start = current.find(BEGIN).unwrap();
    let end = current.find(END).unwrap() + END.len();
    Ok(format!(
        "{}{}",
        current[..start].trim_end_matches(['\r', '\n']),
        &current[end..]
    ))
}
fn patch_catalog(current: &str, target: bool) -> Result<(Option<bool>, Option<String>), String> {
    let mut document = current
        .parse::<DocumentMut>()
        .map_err(|e| format!("invalid Codex config TOML: {e}"))?;
    let previous = document
        .get("skills")
        .and_then(|item| item.get("include_instructions"))
        .and_then(|item| item.as_bool());
    document["skills"]["include_instructions"] = value(target);
    Ok((previous, Some(document.to_string())))
}
fn restore_catalog(current: &str, previous: Option<bool>) -> Result<Option<String>, String> {
    let mut document = current.parse::<DocumentMut>().map_err(|e| e.to_string())?;
    if document
        .get("skills")
        .and_then(|item| item.get("include_instructions"))
        .and_then(|item| item.as_bool())
        != Some(false)
    {
        return Ok(None);
    }
    if let Some(previous) = previous {
        document["skills"]["include_instructions"] = value(previous);
    } else if let Some(skills) = document
        .get_mut("skills")
        .and_then(|item| item.as_table_mut())
    {
        skills.remove("include_instructions");
    }
    Ok(Some(document.to_string()))
}
fn print_plan(config_path: &Path, settings: &Config, catalog: bool) {
    eprintln!(
        "config: {}\ninventory: {:?}\nagent: {:?}\ncatalogue suppression: {}\nsuggestion hook: {:?}",
        config_path.display(),
        settings.inventory,
        settings.agent,
        catalog,
        settings.hooks,
    );
}
fn confirm(yes: bool) -> Result<(), String> {
    if yes {
        return Ok(());
    }
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err("non-interactive init requires --yes".into());
    }
    if inquire::Confirm::new("Apply this plan?")
        .with_default(false)
        .prompt()
        .map_err(|e| e.to_string())?
    {
        Ok(())
    } else {
        Err("setup cancelled".into())
    }
}
fn hash(contents: &[u8]) -> String {
    format!("{:x}", Sha256::digest(contents))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn context_reference_is_idempotent_and_preserves_crlf() {
        let reference = "@/tmp/codex/SKILLWICK.md";
        let once = add_reference("before\r\n", reference).unwrap();
        let twice = add_reference(&once, reference).unwrap();
        assert_eq!(once, twice);
        assert_eq!(reference_count(&twice, reference), 1);
        assert!(twice.contains("\r\n"));
        let removed = remove_reference(&twice, reference, true).unwrap();
        assert_eq!(removed, "before\r\n");
    }
    #[test]
    fn context_reference_is_not_removed_when_not_owned() {
        let reference = "@/tmp/codex/SKILLWICK.md";
        let source = format!("before\n{reference}\nafter\n");
        assert_eq!(remove_reference(&source, reference, false).unwrap(), source);
    }
    #[test]
    fn integration_detection_accepts_previous_and_current_context() {
        let temporary = tempfile::tempdir().unwrap();
        let codex_home = temporary.path();
        let context = codex_home.join("SKILLWICK.md");
        let instructions = codex_home.join("AGENTS.md");
        fs::write(&instructions, format!("@{}\n", context.display())).unwrap();
        let previous = "# Skillwick\n\nSkillwick is a skill helper.\n\n- Run `skillwick --json list --all` to inspect inventory.\n- Read each result with `skillwick read ID`.\n";
        fs::write(&context, previous).unwrap();
        assert!(integration_present(&instructions, codex_home));
        fs::write(&context, CONTEXT).unwrap();
        assert!(integration_present(&instructions, codex_home));
    }
    #[test]
    fn catalog_patch_preserves_unrelated_toml_and_restores_leaf() {
        let source = "model = \"x\"\n[skills]\nmax_context_tokens = 100\n";
        let (_, patched) = patch_catalog(source, false).unwrap();
        let patched = patched.unwrap();
        assert!(patched.contains("model = \"x\""));
        assert!(patched.contains("max_context_tokens = 100"));
        let restored = restore_catalog(&patched, None).unwrap().unwrap();
        assert!(!restored.contains("include_instructions"));
    }
    #[test]
    fn hook_patch_preserves_other_hooks_and_removes_only_its_group() {
        let source = r#"{"description":"owned elsewhere","hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"caveman"}]}],"Stop":[]}}"#;
        let patched = add_hook(source, "'/opt/skillwick' hook").unwrap();
        assert!(patched.contains("caveman"));
        assert!(patched.contains(HOOK_STATUS));
        let restored = remove_hook(&patched, "'/opt/skillwick' hook").unwrap();
        assert!(restored.contains("caveman"));
        assert!(!restored.contains(HOOK_STATUS));
    }
}
