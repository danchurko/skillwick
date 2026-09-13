use crate::{
    config::{Config, Inventory},
    index, native, sources,
};
use rusqlite::{backup::Backup, Connection};
use std::{fmt, path::Path};

#[derive(Debug)]
pub enum Error {
    Database(rusqlite::Error),
    Native(String),
    NoSpecialist,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "database error: {error}"),
            Self::Native(error) => write!(
                formatter,
                "native inventory failed; previous snapshot retained: {error}"
            ),
            Self::NoSpecialist => {
                formatter.write_str("no specialist skill source could be indexed")
            }
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

/// Build the local index and publish it only after every selected source succeeds.
pub fn refresh(
    db: &mut Connection,
    settings: &Config,
    cwd: &Path,
    codex: Option<&native::Codex>,
) -> Result<(), Error> {
    let context = if settings.inventory == Inventory::Codex {
        Some(
            crate::config::normalize_context(cwd, &crate::config::codex_home(settings))
                .map_err(Error::Native)?,
        )
    } else {
        None
    };
    let scan_cwd = context
        .as_ref()
        .map_or(cwd, |context| context.workspace.as_path());
    let scan = sources::scan(scan_cwd, &settings.roots);
    index::refresh_kind(db, "filesystem", &scan.skills, scan.complete)?;
    for diagnostic in scan.diagnostics {
        eprintln!("warning: {diagnostic}");
    }

    let mut has_specialist = scan
        .skills
        .iter()
        .any(|skill| skill.metadata.name != "skillwick");
    if settings.inventory == Inventory::Codex {
        let codex =
            codex.ok_or_else(|| Error::Native("Codex executable was not detected".into()))?;
        let context = context.as_ref().expect("Codex context was normalized");
        let native_skills = native::inventory(codex, &context.codex_home, &context.workspace)
            .map_err(Error::Native)?;
        has_specialist |= native_skills
            .iter()
            .any(|skill| skill.metadata.name != "skillwick");
        index::refresh_kind_for_context(
            db,
            "codex",
            Some((&context.workspace, &context.codex_home)),
            &native_skills,
            true,
        )?;
        index::record_snapshot(
            db,
            &context.workspace,
            &codex.version,
            &codex.path,
            &context.codex_home,
        )?;
    }
    if !has_specialist {
        return Err(Error::NoSpecialist);
    }
    index::publish(db, &crate::config::cache_path())?;
    Ok(())
}

pub fn prime(settings: &Config, codex: &native::Codex, cwd: &Path) -> Result<(), Error> {
    let _lock = index::acquire_cache_lock(&crate::config::cache_path())
        .map_err(|error| Error::Native(format!("cache lock failed: {error}")))?;
    let mut db = match index::open_read_only(&crate::config::cache_path()) {
        Ok(source) => {
            let mut destination = index::open(Path::new(":memory:"))?;
            Backup::new(&source, &mut destination)?.run_to_completion(
                100,
                std::time::Duration::ZERO,
                None,
            )?;
            destination
        }
        Err(_) => index::open(Path::new(":memory:"))?,
    };
    refresh(&mut db, settings, cwd, Some(codex))
}
