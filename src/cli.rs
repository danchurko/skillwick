use crate::{
    config, doctor, evaluation, index, integration, metadata, native, output, search, sources,
};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use rusqlite::{backup::Backup, Connection};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    env, fs,
    io::{self, Read, Write},
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
    #[arg(trailing_var_arg = true)]
    query: Vec<String>,
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
    Benchmark {
        #[arg(long)]
        dataset: PathBuf,
        #[arg(long, short, default_value_t = 5)]
        limit: usize,
        #[arg(long)]
        native_only: bool,
    },
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
        #[arg(long, value_enum, default_value = "off")]
        hooks: HooksArg,
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
    #[command(hide = true)]
    Hook,
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
enum HooksArg {
    Off,
    Suggest,
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
            hooks,
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
                    hooks: match hooks {
                        HooksArg::Off => config::Hooks::Off,
                        HooksArg::Suggest => config::Hooks::Suggest,
                    },
                    roots: root,
                    codex_home,
                    codex_bin,
                    instructions_file,
                },
            )?;
            if !dry_run && initialized.agent == config::Agent::None {
                let mut db = query_database()?;
                refresh(&mut db, &initialized, &cwd, true)?;
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
        Some(Command::Hook) => hook(),
        Some(command @ Command::Benchmark { native_only, .. }) => {
            let settings = config::load(&config_path)?;
            let mut db = query_database()?;
            if native_only && settings.inventory != config::Inventory::Codex {
                return Err(Failure("--native-only requires Codex inventory".into(), 2));
            }
            if native_only {
                index::refresh_kind(&mut db, "filesystem", &[], true)?;
            }
            prepare_index(&mut db, &settings, &cwd, !native_only)?;
            dispatch(
                Some(command),
                args.query,
                args.json,
                &mut db,
                &settings,
                &cwd,
            )?;
        }
        Some(Command::Refresh { full }) => {
            let settings = config::load(&config_path)?;
            let mut db = query_database()?;
            refresh(&mut db, &settings, &cwd, full)?;
        }
        command => {
            let settings = config::load(&config_path)?;
            let mut db = query_database()?;
            prepare_index(&mut db, &settings, &cwd, true)?;
            dispatch(command, args.query, args.json, &mut db, &settings, &cwd)?;
        }
    }
    Ok(())
}

fn dispatch(
    command: Option<Command>,
    trailing: Vec<String>,
    json: bool,
    db: &mut Connection,
    settings: &config::Config,
    cwd: &Path,
) -> Result<(), Failure> {
    match command {
        None => {
            let query = trailing.join(" ");
            if query.is_empty() {
                write_text(&Args::command().render_help().to_string())?;
            } else {
                emit(&search::query(db, &query, 5)?, json)?;
            }
        }
        Some(Command::Search { query, limit }) => {
            emit(
                &search::query(db, &query.join(" "), search_limit(limit)?)?,
                json,
            )?;
        }
        Some(Command::List { limit, all }) => {
            let limit = list_limit(limit)?;
            let rows = search::all(db, limit)?;
            let total = search::count(db)?;
            if json {
                write_output(output::list_json(&rows, total))?;
            } else {
                write_output(output::list_text(&rows, total, all || limit.is_none()))?;
            }
        }
        Some(Command::Inspect { id }) => {
            let row = find(db, &id)?;
            if json {
                write_output(output::json(std::slice::from_ref(&row)))?;
            } else {
                write_text(&format!("id: {}\nname: {}\nscope: {}\nsource: {}\nenabled: {}\nplugin: {}\npath: {}\ncanonical: {}\nbase: {}\nhash: {}\ndegraded: {}\ndescription: {}\n", output::clean(&row.id), output::clean(&row.name), output::clean(&row.scope), output::clean(&row.source), row.enabled, row.plugin_id.as_deref().map(output::clean).unwrap_or_default(), row.path, row.canonical, row.base, row.hash, row.degraded, output::clean(&row.description)))?;
            }
        }
        Some(Command::Read { id }) => read(db, &id, settings, cwd)?,
        Some(Command::Benchmark {
            dataset,
            limit,
            native_only: _,
        }) => benchmark(db, &dataset, search_limit(Some(limit))?, json)?,
        _ => unreachable!(),
    }
    Ok(())
}

fn prepare_index(
    db: &mut Connection,
    settings: &config::Config,
    cwd: &Path,
    filesystem: bool,
) -> Result<(), Failure> {
    if filesystem {
        let scan = sources::scan(cwd, &settings.roots);
        index::refresh_kind(db, "filesystem", &scan.skills, scan.complete)?;
        for diagnostic in scan.diagnostics {
            eprintln!("warning: {diagnostic}");
        }
    }
    if settings.inventory == config::Inventory::Codex && !index::has_kind(db, "codex")? {
        return Err(Failure(
            "native inventory cache is empty; run `skillwick refresh`".into(),
            3,
        ));
    }
    Ok(())
}

fn benchmark(db: &Connection, path: &Path, limit: usize, json: bool) -> Result<(), Failure> {
    let bytes =
        fs::read(path).map_err(|error| Failure(format!("{}: {error}", path.display()), 1))?;
    let dataset: evaluation::Dataset = serde_json::from_slice(&bytes)
        .map_err(|error| Failure(format!("{}: {error}", path.display()), 2))?;
    let rows = search::all(db, None)?;
    let corpus_names: HashSet<_> = rows.iter().map(|row| row.name.clone()).collect();
    let mut corpus_identity: Vec<_> = rows
        .iter()
        .map(|row| format!("{}:{}", row.name, row.hash))
        .collect();
    corpus_identity.sort();
    let report = evaluation::evaluate(
        &dataset,
        &format!("{:x}", Sha256::digest(&bytes)),
        &format!("{:x}", Sha256::digest(corpus_identity.join("\n"))),
        &corpus_names,
        rows.len(),
        limit,
        |query, limit| {
            search::query(db, query, limit)
                .map(|rows| rows.into_iter().map(|row| row.name).collect())
                .map_err(|error| error.to_string())
        },
    )?;
    if json {
        write_text(&format!("{}\n", serde_json::to_string(&report).unwrap()))
    } else {
        write_text(&format!(
            "dataset: {} ({})\ncorpus: {} skills ({})\nretriever: {}\nqueries: {} ({} judged, {} no-match)\nRecall@{limit}: {:.3}\nHitRate@{limit}: {:.3}\nMRR@{limit}: {:.3}\nnDCG@{limit}: {:.3}\nno-match accuracy: {:.3}\nquery latency: p50 {:.3} ms, p95 {:.3} ms\nmisses: {}\n",
            report.dataset,
            &report.dataset_sha256[..12],
            report.corpus_skills,
            &report.corpus_sha256[..12],
            report.retriever,
            report.queries,
            report.judged_queries,
            report.no_match_queries,
            report.recall_at_k,
            report.hit_rate_at_k,
            report.mrr_at_k,
            report.ndcg_at_k,
            report.no_match_accuracy,
            report.query_p50_ms,
            report.query_p95_ms,
            report.misses.len(),
        ))
    }
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
    // ponytail: every refresh hashes metadata; add stat-based skipping only after profiling large libraries.
    let scan = sources::scan(cwd, &settings.roots);
    index::refresh_kind(db, "filesystem", &scan.skills, scan.complete)?;
    for diagnostic in scan.diagnostics {
        eprintln!("warning: {diagnostic}");
    }
    if settings.inventory == config::Inventory::Codex {
        let codex = native::detect(settings)?;
        refresh_native(db, settings, cwd, &codex)?;
    }
    index::publish(db, &config::cache_path())?;
    Ok(())
}

fn refresh_native(
    db: &mut Connection,
    settings: &config::Config,
    cwd: &Path,
    codex: &native::Codex,
) -> Result<(), Failure> {
    match native::inventory(codex, &config::codex_home(settings), cwd) {
        Ok(skills) => {
            index::refresh_kind(db, "codex", &skills, true)?;
            index::record_snapshot(
                db,
                cwd,
                &codex.version,
                &codex.path,
                &config::codex_home(settings),
            )?;
            Ok(())
        }
        Err(error) => Err(Failure(
            format!("native inventory failed; previous snapshot retained: {error}"),
            3,
        )),
    }
}

fn find(db: &Connection, id: &str) -> Result<search::ResultRow, Failure> {
    search::find(db, id)?.ok_or_else(|| Failure("skill not found".into(), 3))
}

fn read(
    db: &mut Connection,
    id: &str,
    settings: &config::Config,
    cwd: &Path,
) -> Result<(), Failure> {
    let mut row = find(db, id)?;
    if row.source_kind == "codex" {
        let codex = native::detect(settings)?;
        refresh_native(db, settings, cwd, &codex)?;
        row = find(db, id)?;
        if !row.enabled {
            return Err(Failure("skill is disabled by Codex".into(), 3));
        }
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
    let body =
        String::from_utf8(bytes).map_err(|_| Failure("instruction file is not UTF-8".into(), 3))?;
    write_text(&format!(
        "path: {}\nbase: {}\n\n{}",
        current.display(),
        row.base,
        body
    ))
}

fn hook() {
    let mut input = Vec::new();
    if io::stdin().take(64 * 1024).read_to_end(&mut input).is_err() {
        return;
    }
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&input) else {
        return;
    };
    let Some(prompt) = value.get("prompt").and_then(|value| value.as_str()) else {
        return;
    };
    let Ok(db) = rusqlite::Connection::open_with_flags(
        config::cache_path(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    ) else {
        return;
    };
    if let Ok(rows) = search::query(&db, prompt, 3) {
        if !rows.is_empty() {
            let mut candidates = String::new();
            for row in rows {
                let description = output::clean(row.description.lines().next().unwrap_or(""));
                let line = format!("{}: {}\n", output::clean(&row.id), description);
                if candidates.len() + line.len() > 1_500 {
                    break;
                }
                candidates.push_str(&line);
            }
            let context = format!(
                "Skillwick found these candidates. Read each relevant ID with `skillwick read ID` before following it; selecting none is valid.\n{candidates}"
            );
            let value = serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "UserPromptSubmit",
                    "additionalContext": context
                }
            });
            let _ = write_text(&format!("{value}\n"));
        }
    }
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
            },
        }
    }

    fn codex_settings() -> config::Config {
        config::Config {
            inventory: config::Inventory::Codex,
            codex_bin: Some(PathBuf::from("/does/not/exist")),
            ..config::Config::default()
        }
    }

    #[test]
    fn cached_queries_do_not_detect_codex() {
        let temp = tempfile::tempdir().unwrap();
        let mut db = index::open(Path::new(":memory:")).unwrap();
        index::refresh_kind(&mut db, "codex", &[codex_skill()], true).unwrap();

        assert!(prepare_index(&mut db, &codex_settings(), temp.path(), false).is_ok());
    }

    #[test]
    fn empty_native_cache_requires_explicit_refresh() {
        let temp = tempfile::tempdir().unwrap();
        let mut db = index::open(Path::new(":memory:")).unwrap();

        let error = match prepare_index(&mut db, &codex_settings(), temp.path(), false) {
            Ok(()) => panic!("empty native cache unexpectedly passed"),
            Err(error) => error,
        };
        assert_eq!(error.code(), 3);
        assert!(error.to_string().contains("run `skillwick refresh`"));
    }

    #[test]
    fn read_only_copy_preserves_native_inventory() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("cache%?#.sqlite3");
        let mut durable = index::open(Path::new(":memory:")).unwrap();
        index::refresh_kind(&mut durable, "codex", &[codex_skill()], true).unwrap();
        index::publish(&durable, &path).unwrap();

        let copied = read_only_copy(&path).unwrap();
        assert!(index::has_kind(&copied, "codex").unwrap());
        assert_eq!(search::count(&copied).unwrap(), 1);
    }
}
