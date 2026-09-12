use crate::{search, sources::Skill};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration};

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    if path != Path::new(":memory:") {
        let parent = path
            .parent()
            .ok_or_else(|| rusqlite::Error::InvalidPath(path.into()))?;
        let parent_existed = parent.exists();
        fs::create_dir_all(parent)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))?;
        #[cfg(unix)]
        if !parent_existed {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))?;
        }
    }
    let db = Connection::open(path)?;
    #[cfg(unix)]
    if path != Path::new(":memory:") {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))?;
    }
    db.busy_timeout(Duration::from_millis(750))?;
    db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS skills (id TEXT PRIMARY KEY, identity TEXT NOT NULL UNIQUE, name TEXT NOT NULL, description TEXT NOT NULL, keywords TEXT NOT NULL, degraded INTEGER NOT NULL, path TEXT NOT NULL, canonical TEXT NOT NULL, base TEXT NOT NULL, scope TEXT NOT NULL, source TEXT NOT NULL, source_kind TEXT NOT NULL, enabled INTEGER NOT NULL, plugin_id TEXT, hash TEXT NOT NULL); CREATE INDEX IF NOT EXISTS skills_kind_canonical ON skills(source_kind, canonical); CREATE VIRTUAL TABLE IF NOT EXISTS skills_fts USING fts5(id UNINDEXED, name, description, keywords); CREATE TABLE IF NOT EXISTS native_snapshots (cwd TEXT PRIMARY KEY, version TEXT NOT NULL, executable TEXT NOT NULL, codex_home TEXT NOT NULL, refreshed_at INTEGER NOT NULL);")?;
    Ok(db)
}

pub fn refresh_kind(
    db: &mut Connection,
    kind: &str,
    skills: &[Skill],
    complete: bool,
) -> rusqlite::Result<()> {
    let transaction = db.transaction()?;
    if complete {
        transaction.execute(
            "DELETE FROM skills_fts WHERE id IN (SELECT id FROM skills WHERE source_kind=?1)",
            [kind],
        )?;
        transaction.execute("DELETE FROM skills WHERE source_kind=?1", [kind])?;
    }
    for skill in skills {
        let identity = format!("{}:{}", skill.source_kind, skill.canonical.display());
        let id = display_id(&transaction, &skill.metadata.name, &identity)?;
        let keywords = format!(
            "{}{}{}",
            skill.metadata.keywords,
            search::alias_terms(&skill.metadata.name),
            search::alias_terms(&skill.metadata.description)
        );
        if !complete {
            let previous_id: Option<String> = transaction
                .query_row(
                    "SELECT id FROM skills WHERE identity=?1",
                    [&identity],
                    |row| row.get(0),
                )
                .optional()?;
            if previous_id
                .as_deref()
                .is_some_and(|previous| previous != id)
            {
                transaction.execute("DELETE FROM skills_fts WHERE id=?1", [previous_id])?;
            }
            transaction.execute("DELETE FROM skills_fts WHERE id=?1", [&id])?;
        }
        transaction.execute("INSERT INTO skills (id,identity,name,description,keywords,degraded,path,canonical,base,scope,source,source_kind,enabled,plugin_id,hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15) ON CONFLICT(identity) DO UPDATE SET id=excluded.id,name=excluded.name,description=excluded.description,keywords=excluded.keywords,degraded=excluded.degraded,path=excluded.path,canonical=excluded.canonical,base=excluded.base,scope=excluded.scope,source=excluded.source,source_kind=excluded.source_kind,enabled=excluded.enabled,plugin_id=excluded.plugin_id,hash=excluded.hash", params![id,identity,skill.metadata.name,skill.metadata.description,keywords,skill.metadata.degraded as i32,skill.path.to_string_lossy(),skill.canonical.to_string_lossy(),skill.base.to_string_lossy(),skill.scope,skill.source,skill.source_kind,skill.enabled as i32,skill.plugin_id,skill.metadata.hash])?;
        transaction.execute(
            "INSERT INTO skills_fts (id,name,description,keywords) VALUES (?1,?2,?3,?4)",
            params![
                id,
                skill.metadata.name,
                skill.metadata.description,
                keywords
            ],
        )?;
    }
    transaction.commit()
}

fn display_id(db: &Connection, name: &str, identity: &str) -> rusqlite::Result<String> {
    let safe_name: String = name
        .chars()
        .filter(|character| !character.is_control() && !character.is_whitespace())
        .collect();
    let digest = format!("{:x}", Sha256::digest(identity.as_bytes()));
    for length in (6..=digest.len()).step_by(2) {
        let id = format!(
            "{}@{}",
            if safe_name.is_empty() {
                "skill"
            } else {
                &safe_name
            },
            &digest[..length]
        );
        let existing: Option<String> = db
            .query_row("SELECT identity FROM skills WHERE id=?1", [&id], |row| {
                row.get(0)
            })
            .optional()?;
        if existing.as_deref().is_none_or(|value| value == identity) {
            return Ok(id);
        }
    }
    unreachable!("SHA-256 collision")
}

pub fn record_snapshot(
    db: &Connection,
    cwd: &Path,
    version: &str,
    executable: &Path,
    codex_home: &Path,
) -> rusqlite::Result<()> {
    db.execute("INSERT INTO native_snapshots (cwd,version,executable,codex_home,refreshed_at) VALUES (?1,?2,?3,?4,unixepoch()) ON CONFLICT(cwd) DO UPDATE SET version=excluded.version,executable=excluded.executable,codex_home=excluded.codex_home,refreshed_at=excluded.refreshed_at", params![cwd.to_string_lossy(), version, executable.to_string_lossy(), codex_home.to_string_lossy()])?;
    Ok(())
}

pub fn has_snapshot(
    db: &Connection,
    cwd: &Path,
    version: &str,
    executable: &Path,
    codex_home: &Path,
) -> rusqlite::Result<bool> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM native_snapshots WHERE cwd=?1 AND version=?2 AND executable=?3 AND codex_home=?4)",
        params![cwd.to_string_lossy(), version, executable.to_string_lossy(), codex_home.to_string_lossy()],
        |row| row.get(0),
    )
}

pub fn has_kind(db: &Connection, kind: &str) -> rusqlite::Result<bool> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM skills WHERE source_kind=?1)",
        [kind],
        |row| row.get(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::Metadata;
    use std::path::PathBuf;
    fn skill(path: &str, description: &str) -> Skill {
        Skill {
            path: path.into(),
            canonical: path.into(),
            base: PathBuf::from(path).parent().unwrap().into(),
            scope: "global".into(),
            source: "/skills".into(),
            source_kind: "filesystem".into(),
            enabled: true,
            plugin_id: None,
            metadata: Metadata {
                name: "demo".into(),
                description: description.into(),
                keywords: String::new(),
                degraded: false,
                hash: description.into(),
            },
        }
    }
    #[test]
    fn incomplete_refresh_preserves_unseen_records() {
        let mut db = open(Path::new(":memory:")).unwrap();
        refresh_kind(
            &mut db,
            "filesystem",
            &[skill("/skills/a/SKILL.md", "a")],
            true,
        )
        .unwrap();
        refresh_kind(&mut db, "filesystem", &[], false).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM skills", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn renamed_skill_replaces_its_fts_row() {
        let mut db = open(Path::new(":memory:")).unwrap();
        let original = skill("/skills/a/SKILL.md", "alpha");
        refresh_kind(&mut db, "filesystem", std::slice::from_ref(&original), true).unwrap();
        let mut renamed = original;
        renamed.metadata.name = "renamed".into();
        refresh_kind(&mut db, "filesystem", &[renamed], false).unwrap();

        assert!(search::query(&db, "demo", 5).unwrap().is_empty());
        assert_eq!(search::query(&db, "renamed", 5).unwrap().len(), 1);
        assert_eq!(
            db.query_row("SELECT count(*) FROM skills_fts", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn reports_cached_source_kinds() {
        let mut db = open(Path::new(":memory:")).unwrap();
        assert!(!has_kind(&db, "codex").unwrap());
        let mut native = skill("/skills/native/SKILL.md", "native");
        native.source_kind = "codex".into();
        refresh_kind(&mut db, "codex", &[native], true).unwrap();
        assert!(has_kind(&db, "codex").unwrap());
    }
}
