use crate::{
    config::{Config, Inventory},
    index, native, sources,
};
use rusqlite::Connection;
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
    let scan = sources::scan(cwd, &settings.roots);
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
        let native_skills = native::inventory(codex, &crate::config::codex_home(settings), cwd)
            .map_err(Error::Native)?;
        has_specialist |= native_skills
            .iter()
            .any(|skill| skill.metadata.name != "skillwick");
        index::refresh_kind(db, "codex", &native_skills, true)?;
        index::record_snapshot(
            db,
            cwd,
            &codex.version,
            &codex.path,
            &crate::config::codex_home(settings),
        )?;
    }
    if !has_specialist {
        return Err(Error::NoSpecialist);
    }
    index::publish(db, &crate::config::cache_path())?;
    Ok(())
}

pub fn prime(settings: &Config, codex: &native::Codex, cwd: &Path) -> Result<(), Error> {
    let mut db = index::open(Path::new(":memory:"))?;
    refresh(&mut db, settings, cwd, Some(codex))
}
