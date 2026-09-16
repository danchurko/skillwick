use crate::{config, doctor, integration, inventory, metadata, output, package, search};
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
    long_about = "Find relevant installed skills without loading an entire catalogue. Search is explicit and bounded. Lookups reconcile installed sources into a disposable local cache."
)]
struct Args {
    #[arg(
        long,
        global = true,
        help = "Emit version-3 JSON for search, list, inspect, read, or doctor"
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
    /// Read selected instructions after validating every requested target.
    ///
    /// This command prints text, does not execute package content, and returns
    /// exit code 3 when the target is missing, ambiguous, stale, or unavailable.
    Read {
        #[arg(
            value_name = "ID|NAME",
            help = "Exact ID returned by search or list, or exact case-sensitive name"
        )]
        #[arg(required = true, num_args = 1..)]
        targets: Vec<String>,
        /// Emit one validated instruction body without metadata.
        #[arg(long, conflicts_with = "json")]
        raw: bool,
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
    /// Output is exhaustive in text or version-3 JSON. The command reads the
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
            help = "Integration target; repeat codex/claude, or use none for no agent files"
        )]
        agent: Vec<AgentArg>,
        /// Choose supported automatic sources or explicit roots only.
        #[arg(long, value_enum)]
        discovery: Option<DiscoveryArg>,
        /// Register automatic sources and integration for this workspace.
        #[arg(long)]
        project: bool,
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
        /// Require a uniquely resolvable skill; repeat for each required skill.
        #[arg(long = "require", value_name = "NAME")]
        required: Vec<String>,
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
        #[arg(value_enum, help = "Shell to generate (bash, zsh, fish)")]
        shell: Shell,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum AgentArg {
    Codex,
    Claude,
    None,
}
#[derive(Clone, Copy, ValueEnum)]
enum Shell {
    Zsh,
    Bash,
    Fish,
}
#[derive(Clone, Copy, ValueEnum)]
enum DiscoveryArg {
    Auto,
    Explicit,
}

pub struct Failure(String, i32);
impl Failure {
    pub fn code(&self) -> i32 {
        self.1
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&output::clean(&self.0))
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
    match &args.command {
        Some(Command::Search { limit, .. }) => {
            search_limit(*limit)?;
        }
        Some(Command::Read { raw: true, targets }) if targets.len() != 1 => {
            return Err(Failure("--raw requires exactly one target".into(), 2));
        }
        _ => {}
    }
    if args.command.is_none() {
        write_text(&Args::command().render_help().to_string())?;
        return Ok(());
    }
    if args.json
        && !matches!(
            args.command.as_ref(),
            Some(Command::Search { .. })
                | Some(Command::Inspect { .. })
                | Some(Command::Read { .. })
                | Some(Command::List)
                | Some(Command::Doctor { .. })
        )
    {
        return Err(Failure(
            "--json is supported only for search, list, inspect, read, and doctor".into(),
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
            discovery,
            project,
            root,
            project_root,
            instructions_file,
        }) => {
            if agent.len() > 1 && agent.iter().any(|a| matches!(a, AgentArg::None)) {
                return Err(Failure(
                    "--agent none cannot be combined with other targets".into(),
                    2,
                ));
            }
            let agents = agent
                .into_iter()
                .map(|agent| match agent {
                    AgentArg::Codex => config::Agent::Codex,
                    AgentArg::Claude => config::Agent::Claude,
                    AgentArg::None => config::Agent::None,
                })
                .collect();
            integration::init(
                &config_path,
                integration::InitRequest {
                    cwd: cwd.clone(),
                    yes,
                    dry_run,
                    agents,
                    roots: root,
                    project_roots: project_root,
                    instructions_file,
                    project,
                    discovery: discovery.map(|value| match value {
                        DiscoveryArg::Auto => config::Discovery::Auto,
                        DiscoveryArg::Explicit => config::Discovery::Explicit,
                    }),
                },
            )
            .map_err(|error| Failure(error.to_string(), error.code()))?;
        }

        Some(Command::Uninstall { purge_cache }) => integration::uninstall(purge_cache)
            .map_err(|error| Failure(error.to_string(), error.code()))?,
        Some(Command::Doctor { strict, required }) => {
            let report = doctor::inspect(&config_path, &cwd, &required)?;
            if args.json {
                write_text(&format!("{}\n", serde_json::to_string(&report).unwrap()))?;
            } else {
                write_output(doctor::text(&report))?;
            }
            if (strict || !required.is_empty()) && !report.healthy {
                return Err(Failure("strict health check failed".into(), 3));
            }
        }
        Some(Command::Completions { shell }) => {
            let shell = match shell {
                Shell::Zsh => clap_complete::Shell::Zsh,
                Shell::Bash => clap_complete::Shell::Bash,
                Shell::Fish => clap_complete::Shell::Fish,
            };
            let mut bytes = Vec::new();
            clap_complete::generate(shell, &mut Args::command(), "skillwick", &mut bytes);
            write_output(io::stdout().lock().write_all(&bytes))?;
        }
        Some(Command::Instructions) => write_text(integration::instructions())?,
        Some(Command::Refresh) => {
            let settings = config::load(&config_path)?;
            refresh(&settings, &cwd)?;
        }
        command => {
            let settings = config::load(&config_path)?;
            let operation = command_name(command.as_ref());
            let mut snapshot =
                inventory::reconcile(&settings, &cwd, operation).map_err(Failure::from)?;
            dispatch(command, args.json, &mut snapshot.db, Some(&snapshot.roots))?;
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
            let total = rows.len();
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
        Some(Command::Read { targets, raw }) => read(db, &targets, raw, json, roots)?,
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

impl From<inventory::Error> for Failure {
    fn from(error: inventory::Error) -> Self {
        let code = match error {
            inventory::Error::Database(_) => 1,
            inventory::Error::Filesystem(_) => 3,
        };
        Failure(error.to_string(), code)
    }
}

fn refresh(settings: &config::Config, cwd: &Path) -> Result<(), Failure> {
    inventory::refresh(settings, cwd).map_err(Failure::from)
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
    Ok(ValidatedSource { bytes })
}

fn inspect_text(row: &search::ResultRow) -> String {
    let mut text = format!("id: {}\nname: {}\nscope: {}\nsource: {}\nenabled: {}\nplugin: {}\npath: {}\ncanonical: {}\nbase: {}\nhash: {}\ndegraded: {}\ndescription: {}\n", output::clean(&row.id), output::clean(&row.name), output::clean(&row.scope), output::clean(&row.source), row.enabled, row.plugin_id.as_deref().map(output::clean).unwrap_or_default(), output::clean(&row.path), output::clean(&row.canonical), output::clean(&row.base), output::clean(&row.hash), row.degraded, output::clean(&row.description));
    for origin in &row.origins {
        text.push_str(&format!(
            "origin: {} [{}] {}\n",
            output::clean(&origin.id),
            output::clean(&origin.scope),
            output::clean(&origin.path)
        ));
    }
    if let Some(diagnostic) = &row.grouping_diagnostic {
        text.push_str(&format!("grouping: {}\n", output::clean(diagnostic)));
    }
    text
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
                    "path": entry.path.clone(),
                    "file_type": entry.file_type,
                    "classification": entry.classification,
                    "extension": entry.extension.clone(),
                })
            })
            .collect();
        let result = serde_json::to_value(row).expect("serializable result");
        let value = serde_json::json!({
            "version": output::JSON_VERSION,
            "results": [result],
            "package": {
                "base": row.base,
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

fn read(
    db: &Connection,
    targets: &[String],
    raw: bool,
    json: bool,
    roots: Option<&[String]>,
) -> Result<(), Failure> {
    if raw && targets.len() != 1 {
        return Err(Failure("--raw requires exactly one target".into(), 2));
    }
    // Resolve and validate every target before emitting any instruction bytes.
    let mut selected = Vec::new();
    for target in targets {
        let (row, resolved_name) = resolve_read(db, target, roots)?;
        let source = validate_source(&row)?;
        let body = String::from_utf8(source.bytes)
            .map_err(|_| Failure("instruction file is not UTF-8".into(), 3))?;
        selected.push((row, resolved_name, body));
    }
    if json {
        let results: Vec<_> = selected
            .into_iter()
            .map(|(row, _, body)| {
                let mut value = serde_json::to_value(row).expect("serializable metadata");
                value["content"] = body.into();
                value
            })
            .collect();
        return write_text(&format!(
            "{}\n",
            serde_json::json!({"version": output::JSON_VERSION, "results": results})
        ));
    }
    let mut text = String::new();
    for (row, resolved_name, body) in selected {
        if !raw {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            if resolved_name {
                text.push_str(&format!("resolved-id: {}\n", output::clean(&row.id)));
            }
            text.push_str(&format!(
                "path: {}\nbase: {}\n\n",
                output::clean(&row.canonical),
                output::clean(&row.base)
            ));
        }
        text.push_str(&body);
    }
    write_text(&text)
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
    fn failures_escape_terminal_controls() {
        let failure = Failure("source\u{1b}[31m\nmissing".into(), 3);
        assert_eq!(failure.to_string(), "source\\u{1b}[31m\\nmissing");
        assert_eq!(failure.code(), 3);
    }

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
            origins: Vec::new(),
            grouping_diagnostic: None,
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
