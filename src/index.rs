use crate::{search, sources::Skill};
use rusqlite::{
    params, params_from_iter, types::Value, Connection, DatabaseName, OpenFlags, OptionalExtension,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

const SCHEMA_VERSION: i64 = 7;

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
    db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE skills (id TEXT PRIMARY KEY, identity TEXT NOT NULL UNIQUE, name TEXT NOT NULL, description TEXT NOT NULL, keywords TEXT NOT NULL, degraded INTEGER NOT NULL, path TEXT NOT NULL, canonical TEXT NOT NULL, base TEXT NOT NULL, scope TEXT NOT NULL, source TEXT NOT NULL, source_kind TEXT NOT NULL, enabled INTEGER NOT NULL, model_discoverable INTEGER NOT NULL, policy_diagnostic TEXT, plugin_id TEXT, hash TEXT NOT NULL, source_fingerprint TEXT NOT NULL); CREATE INDEX skills_kind_canonical ON skills(source_kind, canonical); CREATE TABLE skill_roots (skill_id TEXT NOT NULL, root TEXT NOT NULL, scope TEXT NOT NULL, PRIMARY KEY(skill_id, root, scope)); CREATE INDEX skill_roots_root_scope ON skill_roots(root, scope); CREATE TABLE configured_roots (scope_key TEXT NOT NULL, root TEXT NOT NULL, PRIMARY KEY(scope_key, root)); CREATE VIRTUAL TABLE skills_fts USING fts5(id UNINDEXED, name, description, keywords); PRAGMA user_version=7;")
}

fn validate_schema(db: &Connection) -> rusqlite::Result<()> {
    let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version != SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidQuery);
    }
    db.prepare("SELECT id,identity,name,description,keywords,degraded,path,canonical,base,scope,source,source_kind,enabled,model_discoverable,policy_diagnostic,plugin_id,hash,source_fingerprint FROM skills LIMIT 0")?;
    db.prepare("SELECT skill_id,root,scope FROM skill_roots LIMIT 0")?;
    db.prepare("SELECT scope_key,root FROM configured_roots LIMIT 0")?;
    db.prepare("SELECT id,name,description,keywords FROM skills_fts LIMIT 0")?;
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
    debug_assert_eq!(kind, "filesystem");
    let transaction = db.transaction()?;
    if complete {
        transaction.execute(
            "DELETE FROM skills_fts WHERE id IN (SELECT id FROM skills WHERE source_kind=?1)",
            [kind],
        )?;
        transaction.execute(
            "DELETE FROM skill_roots WHERE skill_id IN (SELECT id FROM skills WHERE source_kind=?1)",
            [kind],
        )?;
        transaction.execute("DELETE FROM skills WHERE source_kind=?1", [kind])?;
    }
    for skill in skills {
        upsert_skill(&transaction, skill)?;
    }
    transaction.commit()
}

/// Refresh only the configured roots in `roots`, preserving rows and
/// associations belonging to other projects in the shared cache.
pub fn refresh_filesystem_scope(
    db: &mut Connection,
    skills: &[Skill],
    roots: &[(PathBuf, String)],
    complete: bool,
) -> rusqlite::Result<()> {
    if complete && scope_matches(db, skills, roots)? {
        return Ok(());
    }
    let transaction = db.transaction()?;
    if complete {
        for (root, scope) in roots {
            transaction.execute(
                "DELETE FROM skill_roots WHERE root=?1 AND scope=?2",
                params![canonical_root(root), scope],
            )?;
        }
    }
    for skill in skills {
        let id = upsert_skill(&transaction, skill)?;
        for (root, scope) in &skill.roots {
            transaction.execute(
                "INSERT OR IGNORE INTO skill_roots (skill_id,root,scope) VALUES (?1,?2,?3)",
                params![id, canonical_root(root), scope],
            )?;
        }
    }
    if complete {
        prune_orphaned_filesystem_rows(&transaction)?;
    }
    transaction.commit()
}

// Compare the complete applicable scan before touching FTS or root associations.
fn scope_matches(
    db: &Connection,
    skills: &[Skill],
    roots: &[(PathBuf, String)],
) -> rusqlite::Result<bool> {
    let mut actual = BTreeSet::new();
    let mut statement = db.prepare("SELECT s.identity,r.root,r.scope FROM skill_roots r JOIN skills s ON s.id=r.skill_id WHERE r.root=?1 AND r.scope=?2")?;
    for (root, scope) in roots {
        let rows = statement.query_map(params![canonical_root(root), scope], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        actual.extend(rows.collect::<rusqlite::Result<Vec<_>>>()?);
    }
    let mut expected = BTreeSet::new();
    for skill in skills {
        let identity = format!("{}:{}", skill.source_kind, skill.canonical.display());
        for (root, scope) in &skill.roots {
            expected.insert((identity.clone(), canonical_root(root), scope.clone()));
        }
        let keywords = format!(
            "{}{}{}",
            skill.metadata.keywords,
            search::alias_terms(&skill.metadata.name),
            search::alias_terms(&skill.metadata.description)
        );
        let matches: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM skills WHERE identity=?1 AND name=?2 AND description=?3 AND keywords=?4 AND degraded=?5 AND path=?6 AND canonical=?7 AND base=?8 AND scope=?9 AND source=?10 AND source_kind=?11 AND enabled=?12 AND model_discoverable=?13 AND policy_diagnostic IS ?14 AND plugin_id IS ?15 AND hash=?16 AND source_fingerprint=?17)", params![identity,skill.metadata.name,skill.metadata.description,keywords,skill.metadata.degraded as i32,skill.path.to_string_lossy(),skill.canonical.to_string_lossy(),skill.base.to_string_lossy(),skill.scope,skill.source,skill.source_kind,skill.enabled as i32,skill.metadata.invocation_policy.model_discoverable() as i32,skill.metadata.policy_diagnostic,skill.plugin_id,skill.metadata.hash,skill.source_fingerprint], |row| row.get(0))?;
        if !matches {
            return Ok(false);
        }
    }
    Ok(actual == expected)
}

pub fn replace_root_configuration(
    db: &mut Connection,
    configured: &[(String, String)],
) -> rusqlite::Result<()> {
    let mut statement = db.prepare("SELECT scope_key,root FROM configured_roots")?;
    let previous = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<BTreeSet<_>>>()?;
    if previous == configured.iter().cloned().collect() {
        return Ok(());
    }
    drop(statement);
    let transaction = db.transaction()?;
    transaction.execute("DELETE FROM configured_roots", [])?;
    for (scope_key, root) in configured {
        transaction.execute(
            "INSERT INTO configured_roots (scope_key,root) VALUES (?1,?2)",
            params![scope_key, root],
        )?;
    }
    transaction.execute(
        "DELETE FROM skill_roots WHERE root NOT IN (SELECT root FROM configured_roots)",
        [],
    )?;
    prune_orphaned_filesystem_rows(&transaction)?;
    transaction.commit()
}

fn upsert_skill(
    transaction: &rusqlite::Transaction<'_>,
    skill: &Skill,
) -> rusqlite::Result<String> {
    let identity = format!("{}:{}", skill.source_kind, skill.canonical.display());
    let id = display_id(transaction, &skill.metadata.name, &identity)?;
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
        let previous = previous_id.as_deref().expect("previous ID was present");
        transaction.execute("DELETE FROM skills_fts WHERE id=?1", [previous])?;
        transaction.execute(
            "UPDATE skill_roots SET skill_id=?1 WHERE skill_id=?2",
            params![id, previous],
        )?;
    }
    let keywords = format!(
        "{}{}{}",
        skill.metadata.keywords,
        search::alias_terms(&skill.metadata.name),
        search::alias_terms(&skill.metadata.description)
    );
    transaction.execute("DELETE FROM skills_fts WHERE id=?1", [&id])?;
    transaction.execute("INSERT INTO skills (id,identity,name,description,keywords,degraded,path,canonical,base,scope,source,source_kind,enabled,model_discoverable,policy_diagnostic,plugin_id,hash,source_fingerprint) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18) ON CONFLICT(identity) DO UPDATE SET id=excluded.id,name=excluded.name,description=excluded.description,keywords=excluded.keywords,degraded=excluded.degraded,path=excluded.path,canonical=excluded.canonical,base=excluded.base,scope=excluded.scope,source=excluded.source,source_kind=excluded.source_kind,enabled=excluded.enabled,model_discoverable=excluded.model_discoverable,policy_diagnostic=excluded.policy_diagnostic,plugin_id=excluded.plugin_id,hash=excluded.hash,source_fingerprint=excluded.source_fingerprint", params![id,identity,skill.metadata.name,skill.metadata.description,keywords,skill.metadata.degraded as i32,skill.path.to_string_lossy(),skill.canonical.to_string_lossy(),skill.base.to_string_lossy(),skill.scope,skill.source,skill.source_kind,skill.enabled as i32,skill.metadata.invocation_policy.model_discoverable() as i32,skill.metadata.policy_diagnostic,skill.plugin_id,skill.metadata.hash,skill.source_fingerprint])?;
    transaction.execute(
        "INSERT INTO skills_fts (id,name,description,keywords) VALUES (?1,?2,?3,?4)",
        params![
            id,
            skill.metadata.name,
            skill.metadata.description,
            keywords
        ],
    )?;
    Ok(id)
}

fn prune_orphaned_filesystem_rows(transaction: &rusqlite::Transaction<'_>) -> rusqlite::Result<()> {
    transaction.execute(
        "DELETE FROM skills_fts WHERE id IN (SELECT s.id FROM skills s WHERE s.source_kind='filesystem' AND NOT EXISTS (SELECT 1 FROM skill_roots r WHERE r.skill_id=s.id))",
        [],
    )?;
    transaction.execute(
        "DELETE FROM skills WHERE source_kind='filesystem' AND NOT EXISTS (SELECT 1 FROM skill_roots r WHERE r.skill_id=skills.id)",
        [],
    )?;
    Ok(())
}

fn canonical_root(root: &Path) -> String {
    fs::canonicalize(root)
        .unwrap_or_else(|_| root.to_path_buf())
        .to_string_lossy()
        .into_owned()
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

#[derive(Debug, Clone, Default, Serialize)]
pub struct Counts {
    pub filesystem: usize,
    pub raw: usize,
    pub duplicates: usize,
    pub model_discoverable: usize,
    pub groups: usize,
    pub verified_copies: usize,
}

pub fn counts(db: &Connection, roots: Option<&[String]>) -> rusqlite::Result<Counts> {
    let mut filesystem_sql =
        "SELECT count(*) FROM skills s WHERE s.source_kind='filesystem'".to_owned();
    filesystem_sql.push_str(&search::root_filter("s", roots, 1));
    let mut root_values = Vec::new();
    search::append_root_values(&mut root_values, roots);
    let filesystem = db.query_row(&filesystem_sql, params_from_iter(root_values), |row| {
        row.get(0)
    })?;
    let raw = match roots {
        Some([]) => 0,
        Some(roots) => {
            let placeholders = (1..=roots.len())
                .map(|index| format!("?{index}"))
                .collect::<Vec<_>>()
                .join(",");
            let values = roots.iter().cloned().map(Value::Text).collect::<Vec<_>>();
            db.query_row(
                &format!("SELECT count(*) FROM skill_roots WHERE root IN ({placeholders})"),
                params_from_iter(values),
                |row| row.get(0),
            )?
        }
        None => filesystem,
    };
    let groups = search::all(db, None, roots)?;
    let model_discoverable = groups
        .iter()
        .flat_map(|row| row.origins.iter().map(|origin| &origin.id))
        .collect::<std::collections::HashSet<_>>()
        .len();
    Ok(Counts {
        filesystem,
        raw,
        duplicates: raw.saturating_sub(filesystem),
        model_discoverable,
        groups: groups.len(),
        verified_copies: model_discoverable.saturating_sub(groups.len()),
    })
}

pub fn digest(db: &Connection) -> rusqlite::Result<String> {
    let mut statement = db.prepare("SELECT id,identity,name,description,keywords,degraded,path,canonical,base,scope,source,source_kind,enabled,model_discoverable,policy_diagnostic,plugin_id,hash,source_fingerprint FROM skills ORDER BY identity")?;
    let mut rows = statement.query([])?;
    let mut hasher = Sha256::new();
    while let Some(row) = rows.next()? {
        for index in 0..18 {
            let value: Option<String> = match index {
                5 | 12 | 13 => row
                    .get::<_, i32>(index)
                    .map(|value| Some(value.to_string()))?,
                _ => row.get(index)?,
            };
            if let Some(value) = value {
                hasher.update(value.as_bytes());
            }
            hasher.update([0]);
        }
    }
    drop(rows);
    drop(statement);
    for sql in [
        "SELECT skill_id,root,scope FROM skill_roots ORDER BY skill_id,root,scope",
        "SELECT scope_key,root,NULL FROM configured_roots ORDER BY scope_key,root",
    ] {
        let mut statement = db.prepare(sql)?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            for index in 0..3 {
                let value: Option<String> = row.get(index)?;
                if let Some(value) = value {
                    hasher.update(value.as_bytes());
                }
                hasher.update([0]);
            }
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn policy_diagnostics(db: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut statement = db.prepare(
        "SELECT path,policy_diagnostic FROM skills WHERE policy_diagnostic IS NOT NULL AND source_kind='filesystem' ORDER BY path",
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
            source_fingerprint: description.into(),
            roots: Vec::new(),
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
    fn unchanged_scope_does_not_write_database_or_fts() {
        let mut db = open(Path::new(":memory:")).unwrap();
        let root = PathBuf::from("/skills");
        let roots = vec![(root.clone(), "global".to_owned())];
        let configured = vec![("shared".to_owned(), "/skills".to_owned())];
        let mut item = skill("/skills/a/SKILL.md", "alpha");
        item.roots = roots.clone();
        replace_root_configuration(&mut db, &configured).unwrap();
        refresh_filesystem_scope(&mut db, &[item.clone()], &roots, true).unwrap();
        let changes = db.total_changes();
        replace_root_configuration(&mut db, &configured).unwrap();
        refresh_filesystem_scope(&mut db, &[item.clone()], &roots, true).unwrap();
        assert_eq!(db.total_changes(), changes);
        item.metadata.description = "updated".into();
        refresh_filesystem_scope(&mut db, &[item], &roots, true).unwrap();
        assert!(db.total_changes() > changes);
        assert_eq!(search::query(&db, "updated", 5, None).unwrap().len(), 1);
        refresh_filesystem_scope(&mut db, &[], &roots, true).unwrap();
        assert!(search::all(&db, None, None).unwrap().is_empty());
    }

    #[test]
    fn complete_refresh_replaces_filesystem_records_and_fts_rows() {
        let mut db = open(Path::new(":memory:")).unwrap();
        refresh_kind(
            &mut db,
            "filesystem",
            &[skill("/skills/a/SKILL.md", "a")],
            true,
        )
        .unwrap();
        let mut renamed = skill("/skills/b/SKILL.md", "b");
        renamed.metadata.name = "renamed".into();
        refresh_kind(&mut db, "filesystem", &[renamed], true).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM skills", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM skills_fts", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(search::query(&db, "renamed", 5, None).unwrap().len() == 1);
    }

    #[test]
    fn digest_changes_for_metadata_and_policy_changes() {
        let mut db = open(Path::new(":memory:")).unwrap();
        let original = skill("/skills/a/SKILL.md", "alpha");
        refresh_kind(&mut db, "filesystem", std::slice::from_ref(&original), true).unwrap();
        let before = digest(&db).unwrap();
        let mut changed = original;
        changed.metadata.description = "changed".into();
        changed.metadata.policy_diagnostic = Some("policy changed".into());
        changed.metadata.invocation_policy = crate::metadata::InvocationPolicy::Denied;
        refresh_kind(&mut db, "filesystem", &[changed], true).unwrap();
        assert_ne!(before, digest(&db).unwrap());
    }

    #[test]
    fn digest_includes_source_fingerprints_and_root_configuration() {
        let mut db = open(Path::new(":memory:")).unwrap();
        let mut record = skill("/skills/a/SKILL.md", "alpha");
        refresh_kind(&mut db, "filesystem", std::slice::from_ref(&record), true).unwrap();
        let before_source = digest(&db).unwrap();
        record.source_fingerprint = "policy-added".into();
        refresh_kind(&mut db, "filesystem", &[record], true).unwrap();
        assert_ne!(before_source, digest(&db).unwrap());

        let before_config = digest(&db).unwrap();
        replace_root_configuration(&mut db, &[("shared".into(), "/configured/root".into())])
            .unwrap();
        assert_ne!(before_config, digest(&db).unwrap());
    }

    #[test]
    fn removed_root_configuration_prunes_only_orphaned_records() {
        let mut db = open(Path::new(":memory:")).unwrap();
        let root_a = PathBuf::from("/roots/a");
        let root_b = PathBuf::from("/roots/b");
        let mut a = skill("/roots/a/one/SKILL.md", "alpha");
        a.roots = vec![(root_a.clone(), "global".into())];
        let mut b = skill("/roots/b/two/SKILL.md", "beta");
        b.roots = vec![(root_b.clone(), "project".into())];
        refresh_filesystem_scope(
            &mut db,
            &[a, b],
            &[
                (root_a.clone(), "global".into()),
                (root_b.clone(), "project".into()),
            ],
            true,
        )
        .unwrap();

        replace_root_configuration(
            &mut db,
            &[("project:/workspace".into(), canonical_root(&root_b))],
        )
        .unwrap();

        assert_eq!(search::count(&db, None).unwrap(), 1);
        assert_eq!(
            db.query_row("SELECT count(*) FROM skill_roots", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }

    #[test]
    fn scoped_refresh_preserves_unrelated_project_records() {
        let mut db = open(Path::new(":memory:")).unwrap();
        let root_a = PathBuf::from("/roots/a");
        let root_b = PathBuf::from("/roots/b");
        let mut a = skill("/skills/a/SKILL.md", "alpha");
        a.metadata.name = "alpha".into();
        a.roots = vec![(root_a.clone(), "project".into())];
        let mut b = skill("/skills/b/SKILL.md", "beta");
        b.metadata.name = "beta".into();
        b.roots = vec![(root_b.clone(), "project".into())];

        refresh_filesystem_scope(
            &mut db,
            std::slice::from_ref(&b),
            &[(root_b.clone(), "project".into())],
            true,
        )
        .unwrap();
        refresh_filesystem_scope(
            &mut db,
            std::slice::from_ref(&a),
            &[(root_a.clone(), "project".into())],
            true,
        )
        .unwrap();

        assert_eq!(
            search::all(&db, None, Some(&[root_a.to_string_lossy().into_owned()]))
                .unwrap()
                .into_iter()
                .map(|row| row.name)
                .collect::<Vec<_>>(),
            vec!["alpha"]
        );
        assert_eq!(
            search::all(&db, None, Some(&[root_b.to_string_lossy().into_owned()]))
                .unwrap()
                .into_iter()
                .map(|row| row.name)
                .collect::<Vec<_>>(),
            vec!["beta"]
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM skills WHERE source_kind='filesystem'",
                [],
                |row| row.get::<_, usize>(0),
            )
            .unwrap(),
            2
        );
    }

    #[test]
    fn reports_raw_and_model_discoverable_counts() {
        let mut db = open(Path::new(":memory:")).unwrap();
        let filesystem = skill("/skills/demo/SKILL.md", "filesystem");
        refresh_kind(&mut db, "filesystem", &[filesystem], true).unwrap();
        let counts = counts(&db, None).unwrap();
        assert_eq!(counts.filesystem, 1);
        assert_eq!(counts.raw, 1);
        assert_eq!(counts.duplicates, 0);
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
