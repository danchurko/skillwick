use crate::{
    config::{self, Agent, Config},
    inventory,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::IsTerminal,
    path::{Path, PathBuf},
};

const BEGIN: &str = "<!-- skillwick:begin -->";
const END: &str = "<!-- skillwick:end -->";
const CONTEXT: &str = include_str!("../assets/skillwick/SKILLWICK.md");
const HOOK_STATUS: &str = "Finding relevant skills with Skillwick";

pub fn instructions() -> &'static str {
    CONTEXT
}

pub struct InitRequest {
    pub cwd: PathBuf,
    pub yes: bool,
    pub dry_run: bool,
    pub agent: Agent,
    pub roots: Vec<PathBuf>,
    pub project_roots: Vec<PathBuf>,
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
        let root = normalize_root(&root)?;
        if !settings.roots.contains(&root) {
            settings.roots.push(root);
        }
    }
    if !request.project_roots.is_empty() {
        let path = config::normalize_cwd(&request.cwd)?;
        let project = settings.projects.iter_mut().find(|project| {
            fs::canonicalize(&project.path)
                .ok()
                .is_some_and(|existing| existing == path)
        });
        if let Some(project) = project {
            for root in request.project_roots {
                let root = normalize_root(&root)?;
                if !project.roots.contains(&root) {
                    project.roots.push(root);
                }
            }
        } else {
            let roots = request
                .project_roots
                .iter()
                .map(|root| normalize_root(root))
                .collect::<Result<Vec<_>, _>>()?;
            settings.projects.push(config::Project { path, roots });
        }
    }
    settings.agent = request.agent;
    if request.instructions_file.is_some() {
        settings.instructions_file = request.instructions_file;
    }
    if settings.agent == Agent::None {
        print_plan(config_path, &settings);
        if request.dry_run {
            return Ok(settings);
        }
        confirm(request.yes)?;
        return Ok(settings);
    }
    let codex_home = config::codex_home();
    let instructions = effective_instructions(&settings, &codex_home)?;
    let hook_file = codex_home.join("hooks.json");
    let previous_journal = fs::read(config::state_dir().join("integration.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Journal>(&bytes).ok());
    let context = context_file(&codex_home)?;
    let reference = reference_line(&context)?;
    print_plan(config_path, &settings);
    if request.dry_run {
        return Ok(settings);
    }
    confirm(request.yes)?;
    config::refuse_symlink(&instructions)?;
    config::refuse_symlink(&context)?;
    if let Some(hook_file) = previous_journal
        .as_ref()
        .and_then(|journal| journal_hook_file(journal, &hook_file))
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
    let hook_retirement = previous_journal.as_ref().and_then(|journal| {
        journal.hook_command.as_ref().and_then(|command| {
            journal_hook_file(journal, &hook_file)
                .map(|path| (path, command.clone(), journal.hook_file_created))
        })
    });
    let hook_updated = if let Some((path, command, file_created)) = &hook_retirement {
        let current = fs::read_to_string(path).unwrap_or_default();
        Some((path.clone(), remove_hook(&current, command)?, *file_created))
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
        router_hash: previous_journal
            .as_ref()
            .and_then(|journal| journal.router_hash.clone()),
        hook_file: None,
        hook_command: None,
        hook_file_created: false,
    };
    prime_index(&settings, &request.cwd).map_err(|error| error.to_string())?;
    if let Some((hook_file, hook_updated, hook_file_created)) = hook_updated {
        if hook_file_created && hook_document_empty(&hook_updated) {
            if hook_file.exists() {
                fs::remove_file(&hook_file).map_err(|e| e.to_string())?;
            }
        } else {
            config::atomic_write(&hook_file, hook_updated.as_bytes(), 0o600)?;
        }
    }
    let journal_path = config::state_dir().join("integration.json");
    config::atomic_write(
        &journal_path,
        &serde_json::to_vec(&journal).map_err(|e| e.to_string())?,
        0o600,
    )?;
    config::save(config_path, &settings)?;
    config::atomic_write(&context, CONTEXT.as_bytes(), 0o600)?;
    config::atomic_write(&instructions, instruction_updated.as_bytes(), 0o600)?;
    if let Some(router) = &legacy_router {
        fs::remove_file(router).map_err(|e| e.to_string())?;
    }
    Ok(settings)
}

fn normalize_root(root: &Path) -> Result<PathBuf, String> {
    let root = fs::canonicalize(root)
        .map_err(|error| format!("cannot normalize skill root {}: {error}", root.display()))?;
    if root.is_dir() {
        Ok(root)
    } else {
        Err(format!("skill root is not a directory: {}", root.display()))
    }
}

fn prime_index(settings: &Config, cwd: &Path) -> Result<(), String> {
    inventory::prime(settings, cwd).map_err(|error| error.to_string())
}

pub fn uninstall(purge_cache: bool) -> Result<(), String> {
    let journal_path = config::state_dir().join("integration.json");
    let journal: Journal = serde_json::from_slice(
        &fs::read(&journal_path).map_err(|e| format!("{}: {e}", journal_path.display()))?,
    )
    .map_err(|e| e.to_string())?;
    let mut drift: Vec<String> = Vec::new();
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
        text.starts_with("# Skillwick\n")
            && text.contains("skillwick --json list")
            && text.contains("skillwick read ID")
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

fn journal_hook_file(journal: &Journal, default: &Path) -> Option<PathBuf> {
    journal.hook_command.as_ref().map(|_| {
        journal
            .hook_file
            .clone()
            .unwrap_or_else(|| default.to_path_buf())
    })
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

fn remove_hook(current: &str, command: &str) -> Result<String, String> {
    let mut document = hook_document(current)?;
    let Some(hooks_value) = document.get_mut("hooks") else {
        return Ok(current.to_string());
    };
    let Some(hooks) = hooks_value.as_object_mut() else {
        return Err("Codex hooks field must be an object".into());
    };
    let Some(event_value) = hooks.get_mut("UserPromptSubmit") else {
        return Ok(current.to_string());
    };
    let Some(event) = event_value.as_array_mut() else {
        return Err("Codex UserPromptSubmit hooks must be an array".into());
    };
    let group = hook_group(command);
    let Some(position) = event.iter().position(|candidate| candidate == &group) else {
        if event.iter().any(|group| {
            group
                .get("hooks")
                .and_then(Value::as_array)
                .is_some_and(|handlers| {
                    handlers.iter().any(|handler| {
                        handler.get("command").and_then(Value::as_str) == Some(command)
                    })
                })
        }) {
            return Err("Skillwick hook changed after setup".into());
        }
        return Ok(current.to_string());
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
fn print_plan(config_path: &Path, settings: &Config) {
    eprintln!(
        "config: {}\nroots: {}\nprojects: {}\nagent: {:?}",
        config_path.display(),
        settings.roots.len(),
        settings.projects.len(),
        settings.agent,
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
    fn hook_retirement_preserves_other_hooks_and_removes_only_its_group() {
        let source = r#"{"description":"owned elsewhere","hooks":{"UserPromptSubmit":[{"hooks":[{"type":"command","command":"caveman"}]}],"Stop":[]}}"#;
        let command = "'/opt/skillwick' hook";
        let mut document: Value = serde_json::from_str(source).unwrap();
        document["hooks"]["UserPromptSubmit"]
            .as_array_mut()
            .unwrap()
            .push(hook_group(command));
        let installed = serde_json::to_string(&document).unwrap();
        let restored = remove_hook(&installed, command).unwrap();
        assert!(restored.contains("caveman"));
        assert!(!restored.contains(HOOK_STATUS));
        assert_eq!(remove_hook(&restored, command).unwrap(), restored);

        let modified = format!(
            r#"{{"hooks":{{"UserPromptSubmit":[{{"hooks":[{{"type":"command","command":"other"}},{{"type":"command","command":"{command}"}}]}}]}}}}"#
        );
        assert!(remove_hook(&modified, command)
            .unwrap_err()
            .contains("changed after setup"));
    }
}
