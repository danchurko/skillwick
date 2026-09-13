use crate::{native, search, sources::Skill};
use rusqlite::{params, Connection, DatabaseName, OpenFlags, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

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
    db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE skills (id TEXT PRIMARY KEY, identity TEXT NOT NULL UNIQUE, name TEXT NOT NULL, description TEXT NOT NULL, keywords TEXT NOT NULL, degraded INTEGER NOT NULL, path TEXT NOT NULL, canonical TEXT NOT NULL, base TEXT NOT NULL, scope TEXT NOT NULL, source TEXT NOT NULL, source_kind TEXT NOT NULL, enabled INTEGER NOT NULL, model_discoverable INTEGER NOT NULL, policy_diagnostic TEXT, plugin_id TEXT, hash TEXT NOT NULL, workspace TEXT NOT NULL, codex_home TEXT NOT NULL); CREATE INDEX skills_kind_canonical ON skills(source_kind, canonical); CREATE VIRTUAL TABLE skills_fts USING fts5(id UNINDEXED, name, description, keywords); CREATE TABLE native_snapshots (workspace TEXT NOT NULL, version TEXT NOT NULL, executable TEXT NOT NULL, codex_home TEXT NOT NULL, refreshed_at INTEGER NOT NULL, PRIMARY KEY (workspace,codex_home)); PRAGMA user_version=3;")
}

fn validate_schema(db: &Connection) -> rusqlite::Result<()> {
    let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version != SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidQuery);
    }
    db.prepare("SELECT id,identity,name,description,keywords,degraded,path,canonical,base,scope,source,source_kind,enabled,model_discoverable,policy_diagnostic,plugin_id,hash,workspace,codex_home FROM skills LIMIT 0")?;
    db.prepare("SELECT id,name,description,keywords FROM skills_fts LIMIT 0")?;
    db.prepare(
        "SELECT workspace,version,executable,codex_home,refreshed_at FROM native_snapshots LIMIT 0",
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
    let context = skills
        .first()
        .and_then(|skill| skill.workspace.as_deref().zip(skill.codex_home.as_deref()));
    refresh_kind_for_context(db, kind, context, skills, complete)
}

pub fn refresh_kind_for_context(
    db: &mut Connection,
    kind: &str,
    context: Option<(&Path, &Path)>,
    skills: &[Skill],
    complete: bool,
) -> rusqlite::Result<()> {
    let transaction = db.transaction()?;
    if complete {
        match context {
            Some((workspace, codex_home)) if kind == "codex" => {
                let workspace = workspace.to_string_lossy();
                let codex_home = codex_home.to_string_lossy();
                transaction.execute(
                    "DELETE FROM skills_fts WHERE id IN (SELECT id FROM skills WHERE source_kind=?1 AND workspace=?2 AND codex_home=?3)",
                    params![kind, workspace.as_ref(), codex_home.as_ref()],
                )?;
                transaction.execute(
                    "DELETE FROM skills WHERE source_kind=?1 AND workspace=?2 AND codex_home=?3",
                    params![kind, workspace.as_ref(), codex_home.as_ref()],
                )?;
            }
            _ => {
                transaction.execute(
                    "DELETE FROM skills_fts WHERE id IN (SELECT id FROM skills WHERE source_kind=?1)",
                    [kind],
                )?;
                transaction.execute("DELETE FROM skills WHERE source_kind=?1", [kind])?;
            }
        }
    }
    for skill in skills {
        let workspace = context
            .map(|(workspace, _)| workspace)
            .or(skill.workspace.as_deref())
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let codex_home = context
            .map(|(_, codex_home)| codex_home)
            .or(skill.codex_home.as_deref())
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let identity = if skill.source_kind == "codex" {
            format!(
                "{}:{}:{}:{}",
                skill.source_kind,
                workspace,
                codex_home,
                skill.canonical.display()
            )
        } else {
            format!("{}:{}", skill.source_kind, skill.canonical.display())
        };
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
        transaction.execute("INSERT INTO skills (id,identity,name,description,keywords,degraded,path,canonical,base,scope,source,source_kind,enabled,model_discoverable,policy_diagnostic,plugin_id,hash,workspace,codex_home) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19) ON CONFLICT(identity) DO UPDATE SET id=excluded.id,name=excluded.name,description=excluded.description,keywords=excluded.keywords,degraded=excluded.degraded,path=excluded.path,canonical=excluded.canonical,base=excluded.base,scope=excluded.scope,source=excluded.source,source_kind=excluded.source_kind,enabled=excluded.enabled,model_discoverable=excluded.model_discoverable,policy_diagnostic=excluded.policy_diagnostic,plugin_id=excluded.plugin_id,hash=excluded.hash,workspace=excluded.workspace,codex_home=excluded.codex_home", params![id,identity,skill.metadata.name,skill.metadata.description,keywords,skill.metadata.degraded as i32,skill.path.to_string_lossy(),skill.canonical.to_string_lossy(),skill.base.to_string_lossy(),skill.scope,skill.source,skill.source_kind,skill.enabled as i32,skill.metadata.invocation_policy.model_discoverable() as i32,skill.metadata.policy_diagnostic,skill.plugin_id,skill.metadata.hash,workspace,codex_home])?;
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
    workspace: &Path,
    version: &str,
    executable: &Path,
    codex_home: &Path,
) -> rusqlite::Result<()> {
    db.execute("INSERT INTO native_snapshots (workspace,version,executable,codex_home,refreshed_at) VALUES (?1,?2,?3,?4,unixepoch()) ON CONFLICT(workspace,codex_home) DO UPDATE SET version=excluded.version,executable=excluded.executable,refreshed_at=excluded.refreshed_at", params![workspace.to_string_lossy(), version, executable.to_string_lossy(), codex_home.to_string_lossy()])?;
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
        "SELECT EXISTS(SELECT 1 FROM native_snapshots WHERE workspace=?1 AND version=?2 AND executable=?3 AND codex_home=?4)",
        params![cwd.to_string_lossy(), version, executable.to_string_lossy(), codex_home.to_string_lossy()],
        |row| row.get(0),
    )
}

pub fn has_workspace_snapshot(
    db: &Connection,
    workspace: &Path,
    codex_home: &Path,
) -> rusqlite::Result<bool> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM native_snapshots WHERE workspace=?1 AND codex_home=?2)",
        params![workspace.to_string_lossy(), codex_home.to_string_lossy()],
        |row| row.get(0),
    )
}

pub fn snapshot_compatible(
    db: &Connection,
    workspace: &Path,
    codex_home: &Path,
    configured_executable: Option<&Path>,
) -> rusqlite::Result<bool> {
    let snapshot: Option<(String, String)> = db
        .query_row(
            "SELECT version,executable FROM native_snapshots WHERE workspace=?1 AND codex_home=?2",
            params![workspace.to_string_lossy(), codex_home.to_string_lossy()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((version, executable)) = snapshot else {
        return Ok(false);
    };
    if !native::supports_native_catalog(&version) {
        return Ok(false);
    }
    if let Some(configured_executable) = configured_executable {
        let Ok(configured_executable) = fs::canonicalize(configured_executable) else {
            return Ok(false);
        };
        if configured_executable != Path::new(&executable) {
            return Ok(false);
        }
    }
    Ok(true)
}

pub struct CacheLock {
    _connection: Connection,
}

pub fn acquire_cache_lock(path: &Path) -> Result<CacheLock, String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("invalid cache path: {}", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    let lock_path = PathBuf::from(format!("{}.lock", path.display()));
    let connection = Connection::open(&lock_path)
        .map_err(|error| format!("{}: {error}", lock_path.display()))?;
    connection
        .busy_timeout(Duration::from_secs(8))
        .and_then(|()| connection.execute_batch("BEGIN EXCLUSIVE"))
        .map_err(|error| format!("{}: {error}", lock_path.display()))?;
    Ok(CacheLock {
        _connection: connection,
    })
}

pub fn has_kind(db: &Connection, kind: &str) -> rusqlite::Result<bool> {
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM skills WHERE source_kind=?1)",
        [kind],
        |row| row.get(0),
    )
}

pub fn has_kind_for_context(
    db: &Connection,
    kind: &str,
    context: Option<&crate::config::Context>,
) -> rusqlite::Result<bool> {
    if kind != "codex" {
        return has_kind(db, kind);
    }
    let Some(context) = context else {
        return has_kind(db, kind);
    };
    db.query_row(
        "SELECT EXISTS(SELECT 1 FROM skills WHERE source_kind=?1 AND workspace=?2 AND codex_home=?3)",
        params![
            kind,
            context.workspace.to_string_lossy(),
            context.codex_home.to_string_lossy()
        ],
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

pub fn counts(
    db: &Connection,
    context: Option<&crate::config::Context>,
) -> rusqlite::Result<Counts> {
    let filesystem = count_kind(db, "filesystem")?;
    let workspace = context.map(|context| context.workspace.to_string_lossy().into_owned());
    let codex_home = context.map(|context| context.codex_home.to_string_lossy().into_owned());
    let native: usize = db.query_row(
        "SELECT count(*) FROM skills WHERE source_kind='codex' AND workspace=?1 AND codex_home=?2",
        params![workspace.as_deref(), codex_home.as_deref()],
        |row| row.get(0),
    )?;
    let raw = filesystem + native;
    let unique: usize = db.query_row(
        "SELECT count(DISTINCT canonical) FROM skills WHERE source_kind='filesystem' OR (source_kind='codex' AND workspace=?1 AND codex_home=?2)",
        params![workspace.as_deref(), codex_home.as_deref()],
        |row| row.get(0),
    )?;
    Ok(Counts {
        filesystem,
        native,
        raw,
        duplicates: raw.saturating_sub(unique),
        model_discoverable: search::count(db, context)?,
    })
}

fn count_kind(db: &Connection, kind: &str) -> rusqlite::Result<usize> {
    db.query_row(
        "SELECT count(*) FROM skills WHERE source_kind=?1",
        [kind],
        |row| row.get(0),
    )
}

pub fn policy_diagnostics(
    db: &Connection,
    context: Option<&crate::config::Context>,
) -> rusqlite::Result<Vec<String>> {
    let workspace = context.map(|context| context.workspace.to_string_lossy().into_owned());
    let codex_home = context.map(|context| context.codex_home.to_string_lossy().into_owned());
    let mut statement = db.prepare(
        "SELECT path,policy_diagnostic FROM skills WHERE policy_diagnostic IS NOT NULL AND (source_kind='filesystem' OR (source_kind='codex' AND workspace=?1 AND codex_home=?2)) ORDER BY path",
    )?;
    let rows = statement
        .query_map(
            params![workspace.as_deref(), codex_home.as_deref()],
            |row| {
                let path: String = row.get(0)?;
                let diagnostic: String = row.get(1)?;
                Ok(format!("{path}: {diagnostic}"))
            },
        )?
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
            workspace: None,
            codex_home: None,
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

        assert!(search::query(&db, None, "demo", 5).unwrap().is_empty());
        assert_eq!(search::query(&db, None, "renamed", 5).unwrap().len(), 1);
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
    fn retains_native_snapshots_for_multiple_contexts() {
        let workspace_a = tempfile::tempdir().unwrap();
        let workspace_b = tempfile::tempdir().unwrap();
        let home_a = tempfile::tempdir().unwrap();
        let home_b = tempfile::tempdir().unwrap();
        let db = open(Path::new(":memory:")).unwrap();

        record_snapshot(
            &db,
            workspace_a.path(),
            "0.154.0",
            Path::new("/codex-a"),
            home_a.path(),
        )
        .unwrap();
        record_snapshot(
            &db,
            workspace_b.path(),
            "0.154.0",
            Path::new("/codex-b"),
            home_b.path(),
        )
        .unwrap();

        assert!(has_workspace_snapshot(&db, workspace_a.path(), home_a.path()).unwrap());
        assert!(has_workspace_snapshot(&db, workspace_b.path(), home_b.path()).unwrap());
    }

    #[test]
    fn reports_raw_duplicate_and_model_discoverable_counts() {
        let workspace = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let mut db = open(Path::new(":memory:")).unwrap();
        let filesystem = skill("/skills/demo/SKILL.md", "filesystem");
        let mut native = filesystem.clone();
        native.source_kind = "codex".into();
        native.source = "codex:native".into();
        refresh_kind(&mut db, "filesystem", &[filesystem], true).unwrap();
        refresh_kind_for_context(
            &mut db,
            "codex",
            Some((workspace.path(), home.path())),
            &[native],
            true,
        )
        .unwrap();
        let context = crate::config::Context {
            workspace: workspace.path().to_path_buf(),
            codex_home: home.path().to_path_buf(),
        };

        let counts = counts(&db, Some(&context)).unwrap();
        assert_eq!(counts.filesystem, 1);
        assert_eq!(counts.native, 1);
        assert_eq!(counts.raw, 2);
        assert_eq!(counts.duplicates, 1);
        assert_eq!(counts.model_discoverable, 1);
    }

    #[test]
    fn counts_only_the_requested_native_partition() {
        let workspace_a = tempfile::tempdir().unwrap();
        let workspace_b = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let mut db = open(Path::new(":memory:")).unwrap();
        let mut native_a = skill("/skills/a/SKILL.md", "a");
        native_a.source_kind = "codex".into();
        let mut native_b = skill("/skills/b/SKILL.md", "b");
        native_b.source_kind = "codex".into();
        refresh_kind_for_context(
            &mut db,
            "codex",
            Some((workspace_a.path(), home.path())),
            &[native_a],
            true,
        )
        .unwrap();
        refresh_kind_for_context(
            &mut db,
            "codex",
            Some((workspace_b.path(), home.path())),
            &[native_b],
            true,
        )
        .unwrap();
        let context = crate::config::Context {
            workspace: workspace_a.path().to_path_buf(),
            codex_home: home.path().to_path_buf(),
        };

        let counts = counts(&db, Some(&context)).unwrap();
        assert_eq!(counts.native, 1);
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

    #[test]
    fn cache_lock_can_be_reacquired_after_owner_drops() {
        let temp = tempfile::tempdir().unwrap();
        let cache = temp.path().join("index.sqlite");
        drop(acquire_cache_lock(&cache).unwrap());
        acquire_cache_lock(&cache).unwrap();
    }
}
