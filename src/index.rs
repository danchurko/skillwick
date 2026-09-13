use crate::{search, sources::Skill};
use rusqlite::{params, Connection, DatabaseName, OpenFlags, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration};

const SCHEMA_VERSION: i64 = 3;

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
    if schema_exists(&db)? {
        validate_schema(&db)?;
    } else {
        create_schema(&db)?;
    }
    Ok(db)
}

pub fn open_read_only(path: &Path) -> rusqlite::Result<Connection> {
    let db = Connection::open_with_flags(
        immutable_uri(path),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?;
    validate_schema(&db)?;
    Ok(db)
}

fn schema_exists(db: &Connection) -> rusqlite::Result<bool> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='skills')",
        [],
        |row| row.get(0),
    )
}

fn create_schema(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE skills (id TEXT PRIMARY KEY, identity TEXT NOT NULL UNIQUE, name TEXT NOT NULL, description TEXT NOT NULL, keywords TEXT NOT NULL, degraded INTEGER NOT NULL, path TEXT NOT NULL, canonical TEXT NOT NULL, base TEXT NOT NULL, scope TEXT NOT NULL, source TEXT NOT NULL, source_kind TEXT NOT NULL, enabled INTEGER NOT NULL, model_discoverable INTEGER NOT NULL, policy_diagnostic TEXT, plugin_id TEXT, hash TEXT NOT NULL); CREATE INDEX skills_kind_canonical ON skills(source_kind, canonical); CREATE VIRTUAL TABLE skills_fts USING fts5(id UNINDEXED, name, description, keywords); CREATE TABLE native_snapshots (cwd TEXT PRIMARY KEY, version TEXT NOT NULL, executable TEXT NOT NULL, codex_home TEXT NOT NULL, refreshed_at INTEGER NOT NULL); PRAGMA user_version=3;")
}

fn validate_schema(db: &Connection) -> rusqlite::Result<()> {
    let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version != SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidQuery);
    }
    db.prepare("SELECT id,identity,name,description,keywords,degraded,path,canonical,base,scope,source,source_kind,enabled,model_discoverable,policy_diagnostic,plugin_id,hash FROM skills LIMIT 0")?;
    db.prepare("SELECT id,name,description,keywords FROM skills_fts LIMIT 0")?;
    db.prepare(
        "SELECT cwd,version,executable,codex_home,refreshed_at FROM native_snapshots LIMIT 0",
    )?;
    Ok(())
}

fn immutable_uri(path: &Path) -> String {
    #[cfg(unix)]
    let bytes = {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes()
    };
    #[cfg(not(unix))]
    let bytes = path.to_string_lossy().as_bytes();
    let mut uri = String::from("file:");
    for byte in bytes {
        let byte = *byte;
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'.' | b'_' | b'~') {
            uri.push(byte as char);
        } else {
            use std::fmt::Write;
            write!(uri, "%{byte:02X}").expect("writing to a string cannot fail");
        }
    }
    uri.push_str("?immutable=1");
    uri
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
        transaction.execute("INSERT INTO skills (id,identity,name,description,keywords,degraded,path,canonical,base,scope,source,source_kind,enabled,model_discoverable,policy_diagnostic,plugin_id,hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17) ON CONFLICT(identity) DO UPDATE SET id=excluded.id,name=excluded.name,description=excluded.description,keywords=excluded.keywords,degraded=excluded.degraded,path=excluded.path,canonical=excluded.canonical,base=excluded.base,scope=excluded.scope,source=excluded.source,source_kind=excluded.source_kind,enabled=excluded.enabled,model_discoverable=excluded.model_discoverable,policy_diagnostic=excluded.policy_diagnostic,plugin_id=excluded.plugin_id,hash=excluded.hash", params![id,identity,skill.metadata.name,skill.metadata.description,keywords,skill.metadata.degraded as i32,skill.path.to_string_lossy(),skill.canonical.to_string_lossy(),skill.base.to_string_lossy(),skill.scope,skill.source,skill.source_kind,skill.enabled as i32,skill.metadata.invocation_policy.model_discoverable() as i32,skill.metadata.policy_diagnostic,skill.plugin_id,skill.metadata.hash])?;
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
    db.execute("DELETE FROM native_snapshots", [])?;
    db.execute("INSERT INTO native_snapshots (cwd,version,executable,codex_home,refreshed_at) VALUES (?1,?2,?3,?4,unixepoch()) ON CONFLICT(cwd) DO UPDATE SET version=excluded.version,executable=excluded.executable,codex_home=excluded.codex_home,refreshed_at=excluded.refreshed_at", params![cwd.to_string_lossy(), version, executable.to_string_lossy(), codex_home.to_string_lossy()])?;
    Ok(())
}

pub fn publish(db: &Connection, path: &Path) -> rusqlite::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| rusqlite::Error::InvalidPath(path.into()))?;
    fs::create_dir_all(parent)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))?;
    let temporary = parent.join(format!(
        ".{}.skillwick-{}",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("snapshot"),
        std::process::id()
    ));
    let result = (|| {
        db.backup(DatabaseName::Main, &temporary, None)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))?;
        }
        fs::File::open(&temporary)
            .and_then(|file| file.sync_all())
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))?;
        fs::rename(&temporary, path)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(error.into()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
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

pub fn has_workspace_snapshot(
    db: &Connection,
    cwd: &Path,
    codex_home: &Path,
) -> rusqlite::Result<bool> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM native_snapshots WHERE cwd=?1 AND codex_home=?2)",
        params![cwd.to_string_lossy(), codex_home.to_string_lossy()],
        |row| row.get(0),
    )
}

pub fn has_codex_home_snapshot(db: &Connection, codex_home: &Path) -> rusqlite::Result<bool> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM native_snapshots WHERE codex_home=?1)",
        [codex_home.to_string_lossy()],
        |row| row.get(0),
    )
}

pub fn has_snapshot_marker(db: &Connection) -> rusqlite::Result<bool> {
    db.query_row("SELECT EXISTS(SELECT 1 FROM native_snapshots)", [], |row| {
        row.get(0)
    })
}

pub fn has_kind(db: &Connection, kind: &str) -> rusqlite::Result<bool> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM skills WHERE source_kind=?1)",
        [kind],
        |row| row.get(0),
    )
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Counts {
    pub filesystem: usize,
    pub native: usize,
    pub raw: usize,
    pub duplicates: usize,
    pub model_discoverable: usize,
}

pub fn counts(db: &Connection) -> rusqlite::Result<Counts> {
    let filesystem = count_kind(db, "filesystem")?;
    let native = count_kind(db, "codex")?;
    let raw: usize = db.query_row("SELECT count(*) FROM skills", [], |row| row.get(0))?;
    let unique: usize =
        db.query_row("SELECT count(DISTINCT canonical) FROM skills", [], |row| {
            row.get(0)
        })?;
    Ok(Counts {
        filesystem,
        native,
        raw,
        duplicates: raw.saturating_sub(unique),
        model_discoverable: search::count(db)?,
    })
}

fn count_kind(db: &Connection, kind: &str) -> rusqlite::Result<usize> {
    db.query_row(
        "SELECT count(*) FROM skills WHERE source_kind=?1",
        [kind],
        |row| row.get(0),
    )
}

pub fn policy_diagnostics(db: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut statement = db.prepare(
        "SELECT path,policy_diagnostic FROM skills WHERE policy_diagnostic IS NOT NULL ORDER BY path",
    )?;
    let rows = statement
        .query_map([], |row| {
            let path: String = row.get(0)?;
            let diagnostic: String = row.get(1)?;
            Ok(format!("{path}: {diagnostic}"))
        })?
        .collect();
    rows
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
                invocation_policy: crate::metadata::InvocationPolicy::Discoverable,
                policy_diagnostic: None,
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

    #[test]
    fn reports_raw_duplicate_and_model_discoverable_counts() {
        let mut db = open(Path::new(":memory:")).unwrap();
        let filesystem = skill("/skills/demo/SKILL.md", "filesystem");
        let mut native = filesystem.clone();
        native.source_kind = "codex".into();
        native.source = "codex:native".into();
        refresh_kind(&mut db, "filesystem", &[filesystem], true).unwrap();
        refresh_kind(&mut db, "codex", &[native], true).unwrap();

        let counts = counts(&db).unwrap();
        assert_eq!(counts.filesystem, 1);
        assert_eq!(counts.native, 1);
        assert_eq!(counts.raw, 2);
        assert_eq!(counts.duplicates, 1);
        assert_eq!(counts.model_discoverable, 1);
    }

    #[test]
    fn rejects_an_old_schema_instead_of_migrating_it() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("index.sqlite");
        let old = Connection::open(&path).unwrap();
        old.execute_batch("CREATE TABLE skills (id TEXT PRIMARY KEY);")
            .unwrap();
        drop(old);
        assert!(open(&path).is_err());
    }
}
