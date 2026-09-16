use crate::{config::Config, discovery, index, sources};
use rusqlite::{backup::Backup, Connection};
use std::{fmt, path::Path, time::Duration};

#[derive(Debug)]
pub enum Error {
    Database(rusqlite::Error),
    Filesystem(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "database error: {error}"),
            Self::Filesystem(error) => write!(formatter, "{error}"),
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

/// A complete, disposable snapshot used by one CLI operation.
///
/// The connection is kept in memory while the durable cache is published only
/// after source discovery and scanning have completed successfully.  `roots`
/// contains the canonical roots applicable to the requested workspace and is
/// used to scope lookups to that workspace.
pub struct Snapshot {
    pub db: Connection,
    pub roots: Vec<String>,
    pub sources: Vec<discovery::SourceSpec>,
    pub diagnostics: Vec<String>,
}

/// Reconcile the configured filesystem roots under the cache lock.
///
/// The returned connection is the same complete snapshot used by the caller,
/// while the durable cache is replaced only when the derived records changed.
pub fn reconcile(settings: &Config, cwd: &Path, operation: &str) -> Result<Snapshot, Error> {
    let cache = crate::config::cache_path();
    let _lock = index::acquire_cache_lock(&cache).map_err(|error| {
        let detail = format!("cache lock failed: {error}");
        Error::Filesystem(failure_message(operation, &detail))
    })?;
    let mut db = read_only_copy(&cache).unwrap_or_else(|_| {
        index::open(Path::new(":memory:")).expect("memory database creation cannot fail")
    });
    let normalized_cwd = crate::config::normalize_cwd(cwd)
        .map_err(|error| Error::Filesystem(failure_message(operation, &error)))?;
    let discovered = discovery::discover(settings, &normalized_cwd);
    for diagnostic in &discovered.diagnostics {
        eprintln!("warning: {diagnostic}");
    }
    if !discovered.complete {
        return Err(Error::Filesystem(failure_message(
            operation,
            &discovered.diagnostics.join("; "),
        )));
    }
    let scan = sources::scan(&normalized_cwd, &discovered);
    for diagnostic in &scan.diagnostics {
        eprintln!("warning: {diagnostic}");
    }
    if !scan.complete {
        return Err(Error::Filesystem(failure_message(
            operation,
            &scan.diagnostics.join("; "),
        )));
    }
    let before = index::digest(&db)?;
    index::replace_root_configuration(&mut db, &discovered.configured_roots)?;
    let applicable_roots = discovered.roots();
    index::refresh_filesystem_scope(&mut db, &scan.skills, &applicable_roots, true)?;
    let changed = before != index::digest(&db)?;
    if changed {
        index::publish(&db, &cache)?;
    }
    Ok(Snapshot {
        db,
        roots: sources::root_keys(&discovered),
        sources: discovered.sources,
        diagnostics: discovered
            .diagnostics
            .into_iter()
            .chain(scan.diagnostics)
            .collect(),
    })
}

fn failure_message(operation: &str, detail: &str) -> String {
    let mut message = format!(
        "filesystem inventory failed during {operation}; previous snapshot retained: {detail}"
    );
    let permission_denied = ["permission denied", "operation not permitted"]
        .iter()
        .any(|needle| detail.to_lowercase().contains(needle));
    if permission_denied {
        message.push_str(&format!(
            "; retry this same `{operation}` operation through the host's supported permission approval when authorized"
        ));
    }
    message
}

pub fn refresh(settings: &Config, cwd: &Path) -> Result<(), Error> {
    reconcile(settings, cwd, "refresh").map(|_| ())
}

pub fn prime(settings: &Config, cwd: &Path) -> Result<(), Error> {
    refresh(settings, cwd)
}

fn read_only_copy(path: &Path) -> rusqlite::Result<Connection> {
    let source = index::open_read_only(path)?;
    let mut destination = index::open(Path::new(":memory:"))?;
    Backup::new(&source, &mut destination)?.run_to_completion(100, Duration::ZERO, None)?;
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::failure_message;

    #[test]
    fn permission_failure_names_bounded_host_recovery() {
        let message = failure_message("list", "Permission denied");
        assert!(message.contains("same `list` operation"));
        assert!(message.contains("host's supported permission approval"));
        assert!(!failure_message("list", "missing root").contains("permission approval"));
    }
}
