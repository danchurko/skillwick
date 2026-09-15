use crate::{
    config, doctor, index, integration, inventory, metadata, output, package, search, sources,
};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

const SEARCH_DEFAULT_LIMIT: usize = 5;
const SEARCH_MAX_LIMIT: usize = 20;

#[derive(Parser)]
#[command(
    name = "skillwick",
    version,
    about = "Find the skill. Load only what matters.",
    long_about = "Find relevant installed skills without loading an entire catalogue. Search is explicit, bounded, local, and read-only until a setup or refresh command is requested."
)]
struct Args {
    #[arg(
        long,
        global = true,
        help = "Emit version-2 JSON for search, list, inspect, or doctor"
    )]
    json: bool,
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        help = "Use PATH as the workspace context (default: current directory)"
    )]
    cwd: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        help = "Read configuration from PATH (default: XDG config directory)"
    )]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Find relevant model-discoverable skills for a task.
    ///
    /// Returns up to five candidates by default. Use --limit to request 1-20
    /// candidates. Exit code 0 also represents a valid no-match result.
    Search {
        #[arg(
            required = true,
            value_name = "QUERY",
            help = "Task words to match; multiple values are joined with spaces"
        )]
        query: Vec<String>,
        #[arg(
            long,
            short,
            value_name = "N",
            help = "Maximum candidates: 1-20 (default: 5); JSON returns all selected records"
        )]
        limit: Option<usize>,
    },
    /// Read one selected instruction file after revalidating its source.
    ///
    /// This command prints text, does not execute package content, and returns
    /// exit code 3 when the target is missing, ambiguous, stale, or unavailable.
    Read {
        #[arg(
            value_name = "ID|NAME",
            help = "Exact ID returned by search or list, or exact case-sensitive name"
        )]
        target: String,
    },
    /// Inspect selected metadata without reading package references.
    ///
    /// Add --files for a bounded live package listing. Source changes and
    /// unavailable packages return exit code 3.
    Inspect {
        #[arg(value_name = "ID", help = "Exact ID returned by search or list")]
        id: String,
        /// List bounded package entries without reading or executing them.
        #[arg(
            long,
            help = "List at most the safe package-entry bound; does not execute files"
        )]
        files: bool,
    },
    /// Show every current-scope model-discoverable skill and its total count.
    ///
    /// Output is exhaustive in text or version-2 JSON. The command reads the
    /// local snapshot and returns an operational error if that snapshot fails.
    List,
    /// Rebuild the disposable local index from configured sources.
    ///
    /// Refresh publishes state only after all configured roots rebuild.
    /// Source failures identify the failed boundary and preserve the last valid
    /// snapshot.
    Refresh,
    /// Print the canonical agent usage instructions.
    ///
    /// Writes Markdown to stdout and performs no indexing or configuration
    /// changes.
    Instructions,
    /// Preview or apply reversible agent integration and filesystem roots.
    ///
    /// Without --yes, non-interactive setup fails with exit code 2. --dry-run
    /// performs no persistent writes.
    Init {
        #[arg(
            long,
            help = "Apply without interactive confirmation; required in non-interactive mode"
        )]
        yes: bool,
        #[arg(
            long,
            help = "Print the plan without writing configuration or the cache"
        )]
        dry_run: bool,
        #[arg(
            long,
            value_enum,
            default_value = "codex",
            help = "Agent integration target (default: codex; use none for no agent files)"
        )]
        agent: AgentArg,
        #[arg(
            long,
            value_name = "PATH",
            help = "Add an authorized filesystem discovery root; repeat as needed"
        )]
        root: Vec<PathBuf>,
        #[arg(
            long,
            value_name = "PATH",
            help = "Add a root associated with the normalized workspace; repeat as needed"
        )]
        project_root: Vec<PathBuf>,
        #[arg(
            long,
            value_name = "PATH",
            help = "Use PATH for the owned agent context reference"
        )]
        instructions_file: Option<PathBuf>,
    },
    /// Diagnose configuration, cache, inventory coverage, and integration health.
    ///
    /// Text diagnostics go to stdout and warnings go to stderr. --strict returns
    /// exit code 3 when the reported state is not healthy.
    Doctor {
        #[arg(long, help = "Return exit code 3 when health is not valid")]
        strict: bool,
    },
    /// Remove only Skillwick-owned integration and optionally its cache.
    Uninstall {
        #[arg(
            long,
            help = "Also remove the disposable local index; installed skills remain untouched"
        )]
        purge_cache: bool,
    },
    /// Generate zsh completion definitions to stdout.
    Completions {
        #[arg(value_enum, help = "Shell to generate (currently: zsh)")]
        shell: Shell,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum AgentArg {
    Codex,
    None,
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
    if args.json
        && !matches!(
            args.command.as_ref(),
            Some(Command::Search { .. })
                | Some(Command::Inspect { .. })
                | Some(Command::List)
                | Some(Command::Doctor { .. })
        )
    {
        return Err(Failure(
            "--json is supported only for search, list, inspect, and doctor".into(),
            2,
        ));
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
            root,
            project_root,
            instructions_file,
        }) => {
            let agent = match agent {
                AgentArg::Codex => config::Agent::Codex,
                AgentArg::None => config::Agent::None,
            };
            let initialized = integration::init(
                &config_path,
                integration::InitRequest {
                    cwd: cwd.clone(),
                    yes,
                    dry_run,
                    agent,
                    roots: root,
                    project_roots: project_root,
                    instructions_file,
                },
            )?;
            if !dry_run && initialized.agent == config::Agent::None {
                refresh(&initialized, &cwd)?;
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
        Some(Command::Refresh) => {
            let settings = config::load(&config_path)?;
            refresh(&settings, &cwd)?;
        }
        command => {
            let settings = config::load(&config_path)?;
            let mut db = index::open(Path::new(":memory:"))?;
            let operation = command_name(command.as_ref());
            prepare_index(&mut db, &settings, &cwd, operation)?;
            let roots = sources::root_keys(&cwd, &settings.roots, &settings.projects);
            dispatch(command, args.json, &mut db, Some(&roots))?;
        }
    }
    Ok(())
}

fn dispatch(
    command: Option<Command>,
    json: bool,
    db: &mut Connection,
    roots: Option<&[String]>,
) -> Result<(), Failure> {
    match command {
        None => unreachable!("missing command handled before index setup"),
        Some(Command::Search { query, limit }) => {
            emit(
                &search::query(db, &query.join(" "), search_limit(limit)?, roots)?,
                json,
            )?;
        }
        Some(Command::List) => {
            let rows = search::all(db, None, roots)?;
            let total = search::count(db, roots)?;
            if json {
                write_output(output::list_json(&rows, total))?;
            } else {
                write_output(output::list_text(&rows, total))?;
            }
        }
        Some(Command::Inspect { id, files }) => {
            let row = find(db, &id, roots)?;
            if files {
                inspect_files(&row, json)?;
            } else if json {
                write_output(output::json(std::slice::from_ref(&row)))?;
            } else {
                write_text(&inspect_text(&row))?;
            }
        }
        Some(Command::Read { target }) => read(db, &target, roots)?,
        _ => unreachable!(),
    }
    Ok(())
}

fn command_name(command: Option<&Command>) -> &'static str {
    match command {
        Some(Command::Search { .. }) => "search",
        Some(Command::List) => "list",
        Some(Command::Read { .. }) => "read",
        Some(Command::Inspect { .. }) => "inspect",
        _ => "lookup",
    }
}

fn prepare_index(
    db: &mut Connection,
    settings: &config::Config,
    cwd: &Path,
    operation: &str,
) -> Result<(), Failure> {
    let refreshed = inventory::reconcile(settings, cwd, operation).map_err(|error| match &error {
        inventory::Error::Database(database) => Failure(
            format!(
                "database error: {database}; run `skillwick refresh` to rebuild the disposable cache"
            ),
            1,
        ),
        inventory::Error::Filesystem(_) => Failure(error.to_string(), 3),
    })?;
    *db = refreshed;
    Ok(())
}

fn refresh(settings: &config::Config, cwd: &Path) -> Result<(), Failure> {
    inventory::refresh(settings, cwd).map_err(|error| match &error {
        inventory::Error::Database(database) => Failure(
            format!(
                "database error: {database}; run `skillwick refresh` to rebuild the disposable cache"
            ),
            1,
        ),
        inventory::Error::Filesystem(_) => {
            Failure(error.to_string(), 3)
        }
    })
}

fn find(db: &Connection, id: &str, roots: Option<&[String]>) -> Result<search::ResultRow, Failure> {
    search::find(db, id, roots)?.ok_or_else(|| Failure("skill not found".into(), 3))
}

fn resolve_read(
    db: &Connection,
    target: &str,
    roots: Option<&[String]>,
) -> Result<(search::ResultRow, bool), Failure> {
    if let Some(row) = search::find(db, target, roots)? {
        return Ok((row, false));
    }
    let mut rows = search::find_name(db, target, roots)?;
    match rows.len() {
        0 => Err(Failure(
            "skill not found; use `skillwick search` to find candidates".into(),
            3,
        )),
        1 => Ok((rows.remove(0), true)),
        _ => {
            let mut error = String::from("skill name is ambiguous; use an exact ID:\n");
            for row in rows {
                error.push_str(&format!(
                    "- {} [source: {}; scope: {}; path: {}]\n",
                    output::clean(&row.id),
                    output::clean(&row.source),
                    output::clean(&row.scope),
                    output::clean(&row.path),
                ));
            }
            Err(Failure(error, 3))
        }
    }
}

struct ValidatedSource {
    path: PathBuf,
    bytes: Vec<u8>,
}

fn validate_source(row: &search::ResultRow) -> Result<ValidatedSource, Failure> {
    if !row.enabled {
        return Err(Failure("skill is disabled".into(), 3));
    }
    let current = fs::canonicalize(&row.path)
        .map_err(|_| Failure("skill source is unavailable".into(), 3))?;
    if current != Path::new(&row.canonical) {
        return Err(Failure(
            "skill source path changed; run `skillwick refresh`".into(),
            3,
        ));
    }
    let bytes = metadata::read_bounded(&current, metadata::MAX_FILE).map_err(|error| {
        if error.is_too_large() {
            Failure("instruction file exceeds 1 MiB".into(), 3)
        } else {
            Failure("skill source is unavailable".into(), 3)
        }
    })?;
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
            "version": output::JSON_VERSION,
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

fn read(db: &Connection, target: &str, roots: Option<&[String]>) -> Result<(), Failure> {
    let (row, resolved_name) = resolve_read(db, target, roots)?;
    let source = validate_source(&row)?;
    let body = String::from_utf8(source.bytes)
        .map_err(|_| Failure("instruction file is not UTF-8".into(), 3))?;
    write_text(&format!(
        "{}path: {}\nbase: {}\n\n{}",
        if resolved_name {
            format!("resolved-id: {}\n", output::clean(&row.id))
        } else {
            String::new()
        },
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
    let limit = limit.unwrap_or(SEARCH_DEFAULT_LIMIT);
    if (1..=SEARCH_MAX_LIMIT).contains(&limit) {
        Ok(limit)
    } else {
        Err(Failure(
            format!("search limit must be between 1 and {SEARCH_MAX_LIMIT}"),
            2,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_limit_accepts_one_through_twenty_with_five_default() {
        assert!(matches!(search_limit(None), Ok(5)));
        assert!(matches!(search_limit(Some(1)), Ok(1)));
        assert!(matches!(search_limit(Some(20)), Ok(20)));
        assert!(matches!(
            search_limit(Some(21)),
            Err(ref failure) if failure.code() == 2
        ));
    }

    #[test]
    fn obsolete_hidden_flags_are_not_parseable() {
        assert!(Args::try_parse_from(["skillwick", "list", "--all"]).is_err());
        assert!(Args::try_parse_from(["skillwick", "list", "--limit", "1"]).is_err());
        assert!(Args::try_parse_from(["skillwick", "refresh", "--full"]).is_err());
        assert!(Args::try_parse_from(["skillwick", "init", "--inventory", "codex"]).is_err());
        assert!(Args::try_parse_from(["skillwick", "init", "--catalog", "native"]).is_err());
        assert!(Args::try_parse_from(["skillwick", "init", "--codex-bin", "/tmp/codex"]).is_err());
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
