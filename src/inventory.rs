use crate::{config::Config, index, sources};
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

/// Reconcile the configured filesystem roots under the cache lock.
///
/// The returned connection is the same complete snapshot used by the caller,
/// while the durable cache is replaced only when the derived records changed.
pub fn reconcile(settings: &Config, cwd: &Path, operation: &str) -> Result<Connection, Error> {
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
    let scan = sources::scan(&normalized_cwd, &settings.roots, &settings.projects);
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
    index::replace_root_configuration(
        &mut db,
        &sources::configured_roots(&settings.roots, &settings.projects),
    )?;
    let applicable_roots = sources::roots(&normalized_cwd, &settings.roots, &settings.projects);
    index::refresh_filesystem_scope(&mut db, &scan.skills, &applicable_roots, true)?;
    let changed = before != index::digest(&db)?;
    if changed {
        index::publish(&db, &cache)?;
    }
    Ok(db)
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
