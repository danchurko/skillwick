use crate::{
    config, doctor, index, integration, inventory, metadata, native, output, package, search,
    sources,
};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use rusqlite::{backup::Backup, Connection};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Parser)]
#[command(
    name = "skillwick",
    version,
    about = "Find the skill. Load only what matters."
)]
struct Args {
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, global = true)]
    cwd: Option<PathBuf>,
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Find relevant skills for a task.
    Search {
        #[arg(required = true)]
        query: Vec<String>,
        #[arg(long, short)]
        limit: Option<usize>,
    },
    Read {
        id: String,
    },
    Inspect {
        id: String,
        /// List bounded package entries without reading or executing them.
        #[arg(long)]
        files: bool,
    },
    /// Show the current-scope skill inventory and total count.
    List {
        #[arg(long, hide = true, conflicts_with = "all")]
        limit: Option<usize>,
        /// Compatibility flag; plain `list` is already exhaustive.
        #[arg(long, hide = true)]
        all: bool,
    },
    Refresh {
        #[arg(long, hide = true)]
        full: bool,
    },
    /// Print the canonical agent usage instructions.
    Instructions,
    Init {
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, value_enum, default_value = "codex")]
        agent: AgentArg,
        #[arg(long, value_enum)]
        inventory: Option<InventoryArg>,
        #[arg(long, value_enum, default_value = "auto")]
        catalog: CatalogArg,
        #[arg(long)]
        root: Vec<PathBuf>,
        #[arg(long)]
        codex_home: Option<PathBuf>,
        #[arg(long)]
        codex_bin: Option<PathBuf>,
        #[arg(long)]
        instructions_file: Option<PathBuf>,
    },
    Doctor {
        #[arg(long)]
        strict: bool,
    },
    Uninstall {
        #[arg(long)]
        purge_cache: bool,
    },
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum AgentArg {
    Codex,
    None,
}
#[derive(Clone, Copy, ValueEnum)]
enum InventoryArg {
    Codex,
    Filesystem,
}
#[derive(Clone, Copy, ValueEnum)]
enum CatalogArg {
    Auto,
    Native,
    Unchanged,
}
#[derive(Clone, Copy, ValueEnum)]
enum Shell {
    Zsh,
}

pub struct Failure(String, i32);
impl Failure {
    pub fn code(&self) -> i32 {
        self.1
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl From<rusqlite::Error> for Failure {
    fn from(error: rusqlite::Error) -> Self {
        Failure(
            format!(
                "database error: {error}; run `skillwick refresh` to rebuild the disposable cache"
            ),
            1,
        )
    }
}
impl From<String> for Failure {
    fn from(error: String) -> Self {
        Failure(error, 1)
    }
}

pub fn run() -> Result<(), Failure> {
    let args = Args::parse();
    if args.command.is_none() {
        write_text(&Args::command().render_help().to_string())?;
        return Ok(());
    }
    let cwd = args
        .cwd
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let config_path = args.config.unwrap_or_else(config::default_path);
    match args.command {
        Some(Command::Init {
            yes,
            dry_run,
            agent,
            inventory,
            catalog,
            root,
            codex_home,
            codex_bin,
            instructions_file,
        }) => {
            let agent = match agent {
                AgentArg::Codex => config::Agent::Codex,
                AgentArg::None => config::Agent::None,
            };
            let inventory = match inventory {
                Some(InventoryArg::Codex) => config::Inventory::Codex,
                Some(InventoryArg::Filesystem) => config::Inventory::Filesystem,
                None if agent == config::Agent::Codex => config::Inventory::Codex,
                None => config::Inventory::Filesystem,
            };
            let initialized = integration::init(
                &config_path,
                integration::InitRequest {
                    cwd: cwd.clone(),
                    yes,
                    dry_run,
                    agent,
                    inventory,
                    catalog: match catalog {
                        CatalogArg::Auto => integration::Catalog::Auto,
                        CatalogArg::Native => integration::Catalog::Native,
                        CatalogArg::Unchanged => integration::Catalog::Unchanged,
                    },
                    roots: root,
                    codex_home,
                    codex_bin,
                    instructions_file,
                },
            )?;
            if !dry_run && initialized.agent == config::Agent::None {
                let _lock =
                    index::acquire_cache_lock(&config::cache_path()).map_err(Failure::from)?;
                let mut db = query_database()?;
                refresh(&mut db, &initialized, &cwd, true)?;
                config::save(&config_path, &initialized)?;
            }
        }
        Some(Command::Uninstall { purge_cache }) => integration::uninstall(purge_cache)?,
        Some(Command::Doctor { strict }) => {
            let report = doctor::inspect(&config_path, &cwd)?;
            if args.json {
                write_text(&format!("{}\n", serde_json::to_string(&report).unwrap()))?;
            } else {
                write_output(doctor::text(&report))?;
            }
            if strict && !report.healthy {
                return Err(Failure("strict health check failed".into(), 3));
            }
        }
        Some(Command::Completions { shell: Shell::Zsh }) => clap_complete::generate(
            clap_complete::Shell::Zsh,
            &mut Args::command(),
            "skillwick",
            &mut io::stdout(),
        ),
        Some(Command::Instructions) => write_text(integration::instructions())?,
        Some(Command::Refresh { full }) => {
            let settings = config::load(&config_path)?;
            let _lock = index::acquire_cache_lock(&config::cache_path()).map_err(Failure::from)?;
            let mut db = query_database()?;
            refresh(&mut db, &settings, &cwd, full)?;
        }
        command => {
            let settings = config::load(&config_path)?;
            let mut db = query_database()?;
            let context = prepare_index(&mut db, &settings, &cwd, true)?;
            dispatch(command, args.json, &mut db, context.as_ref())?;
        }
    }
    Ok(())
}

fn dispatch(
    command: Option<Command>,
    json: bool,
    db: &mut Connection,
    context: Option<&config::Context>,
) -> Result<(), Failure> {
    match command {
        None => unreachable!("missing command handled before index setup"),
        Some(Command::Search { query, limit }) => {
            emit(
                &search::query(db, context, &query.join(" "), search_limit(limit)?)?,
                json,
            )?;
        }
        Some(Command::List { limit, all }) => {
            let limit = list_limit(limit)?;
            let rows = search::all(db, context, limit)?;
            let total = search::count(db, context)?;
            if json {
                write_output(output::list_json(&rows, total))?;
            } else {
                write_output(output::list_text(&rows, total, all || limit.is_none()))?;
            }
        }
        Some(Command::Inspect { id, files }) => {
            let row = find(db, context, &id)?;
            if files {
                inspect_files(&row, json)?;
            } else if json {
                write_output(output::json(std::slice::from_ref(&row)))?;
            } else {
                write_text(&inspect_text(&row))?;
            }
        }
        Some(Command::Read { id }) => read(db, context, &id)?,
        _ => unreachable!(),
    }
    Ok(())
}

fn prepare_index(
    db: &mut Connection,
    settings: &config::Config,
    cwd: &Path,
    filesystem: bool,
) -> Result<Option<config::Context>, Failure> {
    let context = if settings.inventory == config::Inventory::Codex {
        Some(config::normalize_context(cwd, &config::codex_home(settings)).map_err(Failure::from)?)
    } else {
        None
    };
    if filesystem {
        refresh_filesystem(
            db,
            settings,
            context
                .as_ref()
                .map_or(cwd, |context| context.workspace.as_path()),
        )?;
    }
    if let Some(context) = &context {
        if !index::snapshot_compatible(
            db,
            &context.workspace,
            &context.codex_home,
            settings.codex_bin.as_deref(),
        )? {
            eprintln!("notice: native inventory cache misses this context; refreshing");
            *db = auto_refresh(settings, cwd, context)?;
            if filesystem {
                refresh_filesystem(db, settings, &context.workspace)?;
            }
        }
    }
    Ok(context)
}

fn refresh_filesystem(
    db: &mut Connection,
    settings: &config::Config,
    cwd: &Path,
) -> Result<(), Failure> {
    let scan = sources::scan(cwd, &settings.roots);
    index::refresh_kind(db, "filesystem", &scan.skills, scan.complete)?;
    for diagnostic in scan.diagnostics {
        eprintln!("warning: {diagnostic}");
    }
    Ok(())
}

fn query_database() -> Result<Connection, Failure> {
    let cache = config::cache_path();
    match read_only_copy(&cache) {
        Ok(db) => Ok(db),
        Err(error) => {
            if cache.exists() {
                eprintln!("warning: durable cache unavailable ({error}); using empty memory");
            }
            index::open(Path::new(":memory:")).map_err(Into::into)
        }
    }
}

fn auto_refresh(
    settings: &config::Config,
    cwd: &Path,
    context: &config::Context,
) -> Result<Connection, Failure> {
    let _lock = index::acquire_cache_lock(&config::cache_path()).map_err(Failure::from)?;
    let mut db = query_database()?;
    if !index::snapshot_compatible(
        &db,
        &context.workspace,
        &context.codex_home,
        settings.codex_bin.as_deref(),
    )? {
        refresh(&mut db, settings, cwd, true)?;
    }
    Ok(db)
}

fn read_only_copy(path: &Path) -> rusqlite::Result<Connection> {
    let source = index::open_read_only(path)?;
    let mut destination = index::open(Path::new(":memory:"))?;
    Backup::new(&source, &mut destination)?.run_to_completion(100, Duration::ZERO, None)?;
    Ok(destination)
}

fn refresh(
    db: &mut Connection,
    settings: &config::Config,
    cwd: &Path,
    _full: bool,
) -> Result<(), Failure> {
    let codex = if settings.inventory == config::Inventory::Codex {
        Some(native::detect(settings)?)
    } else {
        None
    };
    inventory::refresh(db, settings, cwd, codex.as_ref()).map_err(|error| match error {
        inventory::Error::Database(error) => Failure(
            format!(
                "database error: {error}; run `skillwick refresh` to rebuild the disposable cache"
            ),
            1,
        ),
        inventory::Error::Native(error) => Failure(error.to_string(), 3),
        inventory::Error::NoSpecialist => Failure(error.to_string(), 3),
    })
}

fn find(
    db: &Connection,
    context: Option<&config::Context>,
    id: &str,
) -> Result<search::ResultRow, Failure> {
    search::find(db, context, id)?.ok_or_else(|| Failure("skill not found".into(), 3))
}

struct ValidatedSource {
    path: PathBuf,
    bytes: Vec<u8>,
}

fn validate_source(row: &search::ResultRow) -> Result<ValidatedSource, Failure> {
    if !row.enabled {
        return Err(Failure("skill is disabled by Codex".into(), 3));
    }
    let current = fs::canonicalize(&row.path)
        .map_err(|_| Failure("skill source is unavailable".into(), 3))?;
    if current != Path::new(&row.canonical) {
        return Err(Failure(
            "skill source path changed; run `skillwick refresh`".into(),
            3,
        ));
    }
    let bytes = fs::read(&current).map_err(|_| Failure("skill source is unavailable".into(), 3))?;
    if bytes.len() > metadata::MAX_FILE {
        return Err(Failure("instruction file exceeds 1 MiB".into(), 3));
    }
    if format!("{:x}", Sha256::digest(&bytes)) != row.hash {
        return Err(Failure(
            "skill changed after indexing; run `skillwick refresh`".into(),
            3,
        ));
    }
    Ok(ValidatedSource {
        path: current,
        bytes,
    })
}

fn inspect_text(row: &search::ResultRow) -> String {
    format!("id: {}\nname: {}\nscope: {}\nsource: {}\nenabled: {}\nplugin: {}\npath: {}\ncanonical: {}\nbase: {}\nhash: {}\ndegraded: {}\ndescription: {}\n", output::clean(&row.id), output::clean(&row.name), output::clean(&row.scope), output::clean(&row.source), row.enabled, row.plugin_id.as_deref().map(output::clean).unwrap_or_default(), output::clean(&row.path), output::clean(&row.canonical), output::clean(&row.base), output::clean(&row.hash), row.degraded, output::clean(&row.description))
}

fn inspect_files(row: &search::ResultRow, json: bool) -> Result<(), Failure> {
    let _source = validate_source(row)?;
    let report = package::inspect(Path::new(&row.base))
        .map_err(|error| Failure(format!("skill package is unavailable: {error}"), 3))?;
    if json {
        let entries: Vec<_> = report
            .entries
            .iter()
            .map(|entry| {
                serde_json::json!({
                    "path": output::clean(&entry.path),
                    "file_type": entry.file_type,
                    "classification": entry.classification,
                    "extension": entry.extension.as_deref().map(output::clean),
                })
            })
            .collect();
        let result =
            serde_json::to_value(output::clean_row(row.clone())).expect("serializable result");
        let value = serde_json::json!({
            "version": 1,
            "results": [result],
            "package": {
                "base": output::clean(&row.base),
                "entries": entries,
                "truncated": report.truncated,
                "counts_scope": if report.truncated {
                    "shown_subset"
                } else {
                    "complete"
                },
                "counts": {
                    "total_entries": report.counts.total_entries,
                    "regular_files": report.counts.regular_files,
                    "additional_files": report.counts.additional_files,
                    "markdown_files": report.counts.markdown_files,
                    "non_markdown_files": report.counts.non_markdown_files,
                    "directories": report.counts.directories,
                    "symlinks": report.counts.symlinks,
                    "other": report.counts.other,
                },
                "limits": {
                    "max_entries": package::MAX_ENTRIES,
                    "max_depth": package::MAX_DEPTH,
                    "max_path_bytes": package::MAX_PATH_BYTES,
                },
            },
        });
        write_text(&format!("{value}\n"))
    } else {
        let mut text = inspect_text(row);
        text.push_str(&format!("package: {}\n", output::clean(&row.base)));
        text.push_str(&format!(
            "counts: {}\n",
            if report.truncated {
                "shown subset"
            } else {
                "complete"
            }
        ));
        text.push_str(&format!(
            "entries: {}\nregular files: {}\nadditional regular files: {}\nmarkdown regular files: {}\nnon-markdown regular files: {}\ndirectories: {}\nsymlinks: {}\nother entries: {}\n",
            report.counts.total_entries,
            report.counts.regular_files,
            report.counts.additional_files,
            report.counts.markdown_files,
            report.counts.non_markdown_files,
            report.counts.directories,
            report.counts.symlinks,
            report.counts.other,
        ));
        if report.truncated {
            text.push_str(&format!(
                "listing: {} entries shown (truncated; max {} entries)\n",
                report.counts.total_entries,
                package::MAX_ENTRIES
            ));
        } else {
            text.push_str("listing: complete\n");
        }
        for entry in report.entries {
            let extension = entry
                .extension
                .as_deref()
                .map(|extension| format!(", .{}", output::clean(extension)))
                .unwrap_or_default();
            text.push_str(&format!(
                "- {} [{}; {}{}]\n",
                output::clean(&entry.path),
                output::clean(&entry.classification),
                output::clean(&entry.file_type),
                extension
            ));
        }
        write_text(&text)
    }
}

fn read(db: &Connection, context: Option<&config::Context>, id: &str) -> Result<(), Failure> {
    let row = find(db, context, id)?;
    let source = validate_source(&row)?;
    let body = String::from_utf8(source.bytes)
        .map_err(|_| Failure("instruction file is not UTF-8".into(), 3))?;
    write_text(&format!(
        "path: {}\nbase: {}\n\n{}",
        source.path.display(),
        row.base,
        body
    ))
}

fn emit(rows: &[search::ResultRow], json: bool) -> Result<(), Failure> {
    if json {
        write_output(output::json(rows))
    } else {
        write_output(output::text(rows))
    }
}

fn write_output(result: io::Result<()>) -> Result<(), Failure> {
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(Failure(error.to_string(), 1)),
    }
}

fn write_text(text: &str) -> Result<(), Failure> {
    let stdout = io::stdout();
    write_output(stdout.lock().write_all(text.as_bytes()))
}

fn search_limit(limit: Option<usize>) -> Result<usize, Failure> {
    let limit = limit.unwrap_or(5);
    if (1..=5).contains(&limit) {
        Ok(limit)
    } else {
        Err(Failure("search limit must be between 1 and 5".into(), 2))
    }
}

fn list_limit(limit: Option<usize>) -> Result<Option<usize>, Failure> {
    match limit {
        Some(limit) if limit > 0 => Ok(Some(limit)),
        Some(_) => Err(Failure("list limit must be greater than zero".into(), 2)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{index, metadata::Metadata, sources::Skill};
    use std::path::PathBuf;

    fn codex_skill() -> Skill {
        Skill {
            path: PathBuf::from("/skills/native/SKILL.md"),
            canonical: PathBuf::from("/skills/native/SKILL.md"),
            base: PathBuf::from("/skills/native"),
            workspace: None,
            codex_home: None,
            scope: "global".into(),
            source: "codex:native".into(),
            source_kind: "codex".into(),
            enabled: true,
            plugin_id: None,
            metadata: Metadata {
                name: "native".into(),
                description: "Native skill".into(),
                keywords: String::new(),
                degraded: false,
                hash: "hash".into(),
                invocation_policy: crate::metadata::InvocationPolicy::Discoverable,
                policy_diagnostic: None,
            },
        }
    }

    fn codex_settings_at(home: &Path) -> config::Config {
        config::Config {
            inventory: config::Inventory::Codex,
            codex_home: Some(home.to_path_buf()),
            codex_bin: Some(PathBuf::from("/does/not/exist")),
            ..config::Config::default()
        }
    }

    #[test]
    fn cached_queries_do_not_detect_codex() {
        let temp = tempfile::tempdir().unwrap();
        let codex_home = tempfile::tempdir().unwrap();
        let settings = config::Config {
            codex_home: Some(codex_home.path().to_path_buf()),
            inventory: config::Inventory::Codex,
            ..config::Config::default()
        };
        let context = config::normalize_context(temp.path(), codex_home.path()).unwrap();
        let mut db = index::open(Path::new(":memory:")).unwrap();
        index::refresh_kind(&mut db, "codex", &[codex_skill()], true).unwrap();
        index::record_snapshot(
            &db,
            &context.workspace,
            "0.154.0",
            Path::new("/does/not/exist"),
            &context.codex_home,
        )
        .unwrap();

        assert!(prepare_index(&mut db, &settings, temp.path(), false).is_ok());
    }

    #[test]
    fn cached_queries_require_native_workspace_coverage() {
        let workspace_a = tempfile::tempdir().unwrap();
        let workspace_b = tempfile::tempdir().unwrap();
        let codex_home = tempfile::tempdir().unwrap();
        let settings = codex_settings_at(codex_home.path());
        let context_a = config::normalize_context(workspace_a.path(), codex_home.path()).unwrap();
        let mut skill = codex_skill();
        skill.scope = "project".into();
        let mut db = index::open(Path::new(":memory:")).unwrap();
        index::refresh_kind(&mut db, "codex", &[skill], true).unwrap();
        index::record_snapshot(
            &db,
            &context_a.workspace,
            "0.154.0",
            Path::new("/does/not/exist"),
            &context_a.codex_home,
        )
        .unwrap();

        let error = prepare_index(&mut db, &settings, workspace_b.path(), false)
            .expect_err("uncovered workspace unexpectedly passed");
        assert_eq!(error.code(), 1);
    }

    #[test]
    fn latest_workspace_marker_rejects_older_workspace_queries() {
        let workspace_a = tempfile::tempdir().unwrap();
        let workspace_b = tempfile::tempdir().unwrap();
        let codex_home = tempfile::tempdir().unwrap();
        let settings = config::Config {
            codex_home: Some(codex_home.path().to_path_buf()),
            inventory: config::Inventory::Codex,
            ..config::Config::default()
        };
        let context_a = config::normalize_context(workspace_a.path(), codex_home.path()).unwrap();
        let context_b = config::normalize_context(workspace_b.path(), codex_home.path()).unwrap();
        let mut skill = codex_skill();
        skill.scope = "project".into();
        let mut db = index::open(Path::new(":memory:")).unwrap();
        index::refresh_kind(&mut db, "codex", &[skill], true).unwrap();
        index::record_snapshot(
            &db,
            &context_a.workspace,
            "0.154.0",
            Path::new("/does/not/exist"),
            &context_a.codex_home,
        )
        .unwrap();
        index::record_snapshot(
            &db,
            &context_b.workspace,
            "0.154.0",
            Path::new("/does/not/exist"),
            &context_b.codex_home,
        )
        .unwrap();

        assert!(prepare_index(&mut db, &settings, workspace_a.path(), false).is_ok());
    }

    #[test]
    fn another_codex_home_requires_refresh() {
        let workspace = tempfile::tempdir().unwrap();
        let codex_home_a = tempfile::tempdir().unwrap();
        let codex_home_b = tempfile::tempdir().unwrap();
        let mut db = index::open(Path::new(":memory:")).unwrap();
        index::refresh_kind(&mut db, "codex", &[codex_skill()], true).unwrap();
        index::record_snapshot(
            &db,
            workspace.path(),
            "0.154.0",
            Path::new("/does/not/exist"),
            codex_home_a.path(),
        )
        .unwrap();
        let settings = codex_settings_at(codex_home_b.path());

        let error = prepare_index(&mut db, &settings, workspace.path(), false).unwrap_err();
        assert_eq!(error.code(), 1);
    }

    #[test]
    fn empty_native_cache_requires_explicit_refresh() {
        let temp = tempfile::tempdir().unwrap();
        let codex_home = tempfile::tempdir().unwrap();
        let settings = codex_settings_at(codex_home.path());
        let mut db = index::open(Path::new(":memory:")).unwrap();

        let error = match prepare_index(&mut db, &settings, temp.path(), false) {
            Ok(_) => panic!("empty native cache unexpectedly passed"),
            Err(error) => error,
        };
        assert_eq!(error.code(), 1);
    }

    #[test]
    fn read_only_copy_preserves_native_inventory() {
        let temp = tempfile::tempdir().unwrap();
        let codex_home = tempfile::tempdir().unwrap();
        let path = temp.path().join("cache%?#.sqlite3");
        let mut durable = index::open(Path::new(":memory:")).unwrap();
        index::refresh_kind_for_context(
            &mut durable,
            "codex",
            Some((temp.path(), codex_home.path())),
            &[codex_skill()],
            true,
        )
        .unwrap();
        index::publish(&durable, &path).unwrap();

        let copied = read_only_copy(&path).unwrap();
        assert!(index::has_kind(&copied, "codex").unwrap());
        let context = config::Context {
            workspace: temp.path().to_path_buf(),
            codex_home: codex_home.path().to_path_buf(),
        };
        assert_eq!(search::count(&copied, Some(&context)).unwrap(), 1);
    }

    #[test]
    fn read_uses_published_native_policy_without_starting_codex() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("SKILL.md");
        let body = b"---\nname: native\ndescription: Native skill\n---\n";
        fs::write(&path, body).unwrap();
        let mut skill = codex_skill();
        skill.path = path.clone();
        skill.canonical = fs::canonicalize(&path).unwrap();
        skill.base = temp.path().to_path_buf();
        skill.metadata.hash = format!("{:x}", Sha256::digest(body));
        let mut db = index::open(Path::new(":memory:")).unwrap();
        index::refresh_kind_for_context(
            &mut db,
            "codex",
            Some((temp.path(), temp.path())),
            &[skill],
            true,
        )
        .unwrap();
        let context = config::Context {
            workspace: temp.path().to_path_buf(),
            codex_home: temp.path().to_path_buf(),
        };
        let id = search::all(&db, Some(&context), None).unwrap().remove(0).id;

        assert!(read(&db, Some(&context), &id).is_ok());
    }

    #[test]
    fn inspect_text_sanitizes_provenance_fields() {
        let unsafe_text = "safe\u{1b}[31m\n".to_owned();
        let row = search::ResultRow {
            id: "id".into(),
            name: "name".into(),
            description: "description".into(),
            scope: "scope".into(),
            path: unsafe_text.clone(),
            canonical: unsafe_text.clone(),
            base: unsafe_text.clone(),
            source: "source".into(),
            source_kind: "filesystem".into(),
            enabled: true,
            plugin_id: None,
            degraded: false,
            hash: unsafe_text,
        };

        let rendered = inspect_text(&row);
        assert!(!rendered.contains('\u{1b}'));
        for prefix in ["path:", "canonical:", "base:", "hash:"] {
            assert!(rendered
                .lines()
                .find(|line| line.starts_with(prefix))
                .is_some_and(|line| !line.contains('\u{1b}')));
        }
    }
}
