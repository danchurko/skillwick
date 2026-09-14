use crate::config::Context;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::cmp::Ordering;

#[derive(Debug, Clone, Serialize)]
pub struct ResultRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub scope: String,
    pub path: String,
    pub canonical: String,
    pub base: String,
    pub source: String,
    pub source_kind: String,
    pub enabled: bool,
    pub plugin_id: Option<String>,
    pub degraded: bool,
    pub hash: String,
}

struct Candidate {
    row: ResultRow,
    score: f64,
    exact: bool,
    coverage: usize,
    search_text: String,
}

pub fn tokens(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .filter_map(|raw| {
            let clean = raw.trim_matches(|character: char| {
                !character.is_alphanumeric() && !"+#.-".contains(character)
            });
            if clean.is_empty() {
                return None;
            }
            let lower = clean.to_lowercase();
            if matches!(
                lower.as_str(),
                "a" | "an" | "and" | "about" | "for" | "in" | "of" | "on" | "the" | "to" | "with"
            ) {
                return None;
            }
            Some(match lower.as_str() {
                "c++" => "cpp".into(),
                "c#" => "csharp".into(),
                ".net" => "dotnet".into(),
                "node.js" => "nodejs".into(),
                _ => lower,
            })
        })
        .collect()
}

fn expression(tokens: &[String]) -> String {
    tokens
        .iter()
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

pub fn query(
    db: &Connection,
    context: Option<&Context>,
    query: &str,
    limit: usize,
) -> rusqlite::Result<Vec<ResultRow>> {
    let query_tokens = tokens(query);
    if query_tokens.is_empty() {
        return Ok(Vec::new());
    }
    let mut statement = db.prepare("SELECT s.id,s.name,s.description,s.scope,s.path,s.canonical,s.base,s.source,s.source_kind,s.enabled,s.plugin_id,s.degraded,s.hash,bm25(skills_fts,8.0,3.0,1.0),lower(s.name)||' '||lower(s.description)||' '||lower(s.keywords) FROM skills_fts JOIN skills s ON s.id=skills_fts.id WHERE skills_fts MATCH ?1 AND s.enabled=1 AND s.model_discoverable=1 AND (s.source_kind='filesystem' OR (s.source_kind='codex' AND s.workspace=?2 AND s.codex_home=?3)) AND NOT (s.source_kind='filesystem' AND EXISTS (SELECT 1 FROM skills n WHERE n.source_kind='codex' AND n.workspace=?2 AND n.codex_home=?3 AND n.canonical=s.canonical))")?;
    let workspace = context.map(|context| context.workspace.to_string_lossy().into_owned());
    let codex_home = context.map(|context| context.codex_home.to_string_lossy().into_owned());
    let mapped = statement.query_map(
        params![
            expression(&query_tokens),
            workspace.as_deref(),
            codex_home.as_deref()
        ],
        |row| {
            let result = row_from(row)?;
            let score = row.get(13)?;
            let search_text: String = row.get(14)?;
            let exact = result.name.to_lowercase() == query.trim().to_lowercase();
            let coverage = query_tokens
                .iter()
                .filter(|token| search_text.contains(token.as_str()))
                .count();
            Ok(Candidate {
                row: result,
                score,
                exact,
                coverage,
                search_text,
            })
        },
    )?;
    let minimum_coverage = if query_tokens.len() >= 3 { 2 } else { 1 };
    let mut candidates: Vec<_> = mapped
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|candidate| candidate.exact || candidate.coverage >= minimum_coverage)
        .collect();
    candidates.sort_by(|left, right| {
        right
            .exact
            .cmp(&left.exact)
            .then(right.coverage.cmp(&left.coverage))
            .then_with(|| {
                left.score
                    .partial_cmp(&right.score)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| {
                left.row
                    .name
                    .to_lowercase()
                    .cmp(&right.row.name.to_lowercase())
            })
            .then(left.row.id.cmp(&right.row.id))
    });
    Ok(candidates
        .into_iter()
        .take(limit)
        .map(|candidate| {
            let _ = candidate.search_text;
            candidate.row
        })
        .collect())
}

pub fn all(
    db: &Connection,
    context: Option<&Context>,
    limit: Option<usize>,
) -> rusqlite::Result<Vec<ResultRow>> {
    let workspace = context.map(|context| context.workspace.to_string_lossy().into_owned());
    let codex_home = context.map(|context| context.codex_home.to_string_lossy().into_owned());
    let sql = if limit.is_some() {
        "SELECT s.id,s.name,s.description,s.scope,s.path,s.canonical,s.base,s.source,s.source_kind,s.enabled,s.plugin_id,s.degraded,s.hash FROM skills s WHERE s.enabled=1 AND s.model_discoverable=1 AND (s.source_kind='filesystem' OR (s.source_kind='codex' AND s.workspace=?1 AND s.codex_home=?2)) AND NOT (s.source_kind='filesystem' AND EXISTS (SELECT 1 FROM skills n WHERE n.source_kind='codex' AND n.workspace=?1 AND n.codex_home=?2 AND n.canonical=s.canonical)) ORDER BY lower(s.name),s.id LIMIT ?3"
    } else {
        "SELECT s.id,s.name,s.description,s.scope,s.path,s.canonical,s.base,s.source,s.source_kind,s.enabled,s.plugin_id,s.degraded,s.hash FROM skills s WHERE s.enabled=1 AND s.model_discoverable=1 AND (s.source_kind='filesystem' OR (s.source_kind='codex' AND s.workspace=?1 AND s.codex_home=?2)) AND NOT (s.source_kind='filesystem' AND EXISTS (SELECT 1 FROM skills n WHERE n.source_kind='codex' AND n.workspace=?1 AND n.codex_home=?2 AND n.canonical=s.canonical)) ORDER BY lower(s.name),s.id"
    };
    let mut statement = db.prepare(sql)?;
    if let Some(limit) = limit {
        statement
            .query_map(
                params![workspace.as_deref(), codex_home.as_deref(), limit as i64],
                row_from,
            )?
            .collect()
    } else {
        statement
            .query_map(
                params![workspace.as_deref(), codex_home.as_deref()],
                row_from,
            )?
            .collect()
    }
}

pub fn count(db: &Connection, context: Option<&Context>) -> rusqlite::Result<usize> {
    let workspace = context.map(|context| context.workspace.to_string_lossy().into_owned());
    let codex_home = context.map(|context| context.codex_home.to_string_lossy().into_owned());
    db.query_row(
        "SELECT count(*) FROM skills s WHERE s.enabled=1 AND s.model_discoverable=1 AND (s.source_kind='filesystem' OR (s.source_kind='codex' AND s.workspace=?1 AND s.codex_home=?2)) AND NOT (s.source_kind='filesystem' AND EXISTS (SELECT 1 FROM skills n WHERE n.source_kind='codex' AND n.workspace=?1 AND n.codex_home=?2 AND n.canonical=s.canonical))",
        params![workspace.as_deref(), codex_home.as_deref()],
        |row| row.get(0),
    )
}

pub fn find(
    db: &Connection,
    context: Option<&Context>,
    id: &str,
) -> rusqlite::Result<Option<ResultRow>> {
    let workspace = context.map(|context| context.workspace.to_string_lossy().into_owned());
    let codex_home = context.map(|context| context.codex_home.to_string_lossy().into_owned());
    let mut statement = db.prepare("SELECT s.id,s.name,s.description,s.scope,s.path,s.canonical,s.base,s.source,s.source_kind,s.enabled,s.plugin_id,s.degraded,s.hash FROM skills s WHERE s.id=?1 AND s.enabled=1 AND s.model_discoverable=1 AND (s.source_kind='filesystem' OR (s.source_kind='codex' AND s.workspace=?2 AND s.codex_home=?3)) AND NOT (s.source_kind='filesystem' AND EXISTS (SELECT 1 FROM skills n WHERE n.source_kind='codex' AND n.workspace=?2 AND n.codex_home=?3 AND n.canonical=s.canonical))")?;
    let mut rows = statement.query(params![id, workspace.as_deref(), codex_home.as_deref()])?;
    rows.next()?.map(row_from).transpose()
}

pub fn find_name(
    db: &Connection,
    context: Option<&Context>,
    name: &str,
) -> rusqlite::Result<Vec<ResultRow>> {
    let workspace = context.map(|context| context.workspace.to_string_lossy().into_owned());
    let codex_home = context.map(|context| context.codex_home.to_string_lossy().into_owned());
    let mut statement = db.prepare("SELECT s.id,s.name,s.description,s.scope,s.path,s.canonical,s.base,s.source,s.source_kind,s.enabled,s.plugin_id,s.degraded,s.hash FROM skills s WHERE s.name=?1 AND s.enabled=1 AND s.model_discoverable=1 AND (s.source_kind='filesystem' OR (s.source_kind='codex' AND s.workspace=?2 AND s.codex_home=?3)) AND NOT (s.source_kind='filesystem' AND EXISTS (SELECT 1 FROM skills n WHERE n.source_kind='codex' AND n.workspace=?2 AND n.codex_home=?3 AND n.canonical=s.canonical)) ORDER BY s.id")?;
    let rows = statement
        .query_map(
            params![name, workspace.as_deref(), codex_home.as_deref()],
            row_from,
        )?
        .collect();
    rows
}

fn row_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResultRow> {
    Ok(ResultRow {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        scope: row.get(3)?,
        path: row.get(4)?,
        canonical: row.get(5)?,
        base: row.get(6)?,
        source: row.get(7)?,
        source_kind: row.get(8)?,
        enabled: row.get::<_, i32>(9)? != 0,
        plugin_id: row.get(10)?,
        degraded: row.get::<_, i32>(11)? != 0,
        hash: row.get(12)?,
    })
}

pub fn alias_terms(text: &str) -> &'static str {
    let lower = text.to_lowercase();
    match (
        lower.contains("c++"),
        lower.contains("c#"),
        lower.contains(".net"),
        lower.contains("node.js"),
    ) {
        (false, false, false, false) => "",
        (true, false, false, false) => " cpp",
        (false, true, false, false) => " csharp",
        (false, false, true, false) => " dotnet",
        (false, false, false, true) => " nodejs",
        _ => " cpp csharp dotnet nodejs",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{index, metadata::Metadata, sources::Skill};
    use std::path::{Path, PathBuf};

    fn fixture_skill(name: &str, description: &str) -> Skill {
        Skill {
            path: PathBuf::from(format!("/{name}/SKILL.md")),
            canonical: PathBuf::from(format!("/{name}/SKILL.md")),
            base: PathBuf::from(format!("/{name}")),
            workspace: None,
            codex_home: None,
            scope: "global".into(),
            source: format!("/{name}"),
            source_kind: "filesystem".into(),
            enabled: true,
            plugin_id: None,
            metadata: Metadata {
                name: name.into(),
                description: description.into(),
                keywords: String::new(),
                degraded: false,
                hash: "hash".into(),
                invocation_policy: crate::metadata::InvocationPolicy::Discoverable,
                policy_diagnostic: None,
            },
        }
    }

    #[test]
    fn ranks_exact_and_handles_technical_tokens_stably() {
        let mut db = index::open(Path::new(":memory:")).unwrap();
        let make = |name: &str, description: &str| Skill {
            path: PathBuf::from(format!("/{name}/SKILL.md")),
            canonical: PathBuf::from(format!("/{name}/SKILL.md")),
            base: PathBuf::from(format!("/{name}")),
            workspace: None,
            codex_home: None,
            scope: "global".into(),
            source: format!("/{name}"),
            source_kind: "filesystem".into(),
            enabled: true,
            plugin_id: None,
            metadata: Metadata {
                name: name.into(),
                description: description.into(),
                keywords: String::new(),
                degraded: false,
                hash: "hash".into(),
                invocation_policy: crate::metadata::InvocationPolicy::Discoverable,
                policy_diagnostic: None,
            },
        };
        index::refresh_kind(
            &mut db,
            "filesystem",
            &[
                make("C++", "Native tools"),
                make("other", "Generic C language"),
            ],
            true,
        )
        .unwrap();
        let first = query(&db, None, "C++", 5).unwrap();
        let second = query(&db, None, "C++", 5).unwrap();
        assert_eq!(first[0].name, "C++");
        assert_eq!(
            first.iter().map(|row| &row.id).collect::<Vec<_>>(),
            second.iter().map(|row| &row.id).collect::<Vec<_>>()
        );
        assert!(query(&db, None, "quoted \" text", 5).is_ok());
        assert_eq!(count(&db, None).unwrap(), 2);
    }

    #[test]
    fn exact_name_lookup_is_case_sensitive() {
        let mut db = index::open(Path::new(":memory:")).unwrap();
        let skill = fixture_skill("Readable Name", "An exact-name fixture");
        index::refresh_kind(&mut db, "filesystem", &[skill], true).unwrap();

        let rows = find_name(&db, None, "Readable Name").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Readable Name");
        assert!(find_name(&db, None, "readable name").unwrap().is_empty());
    }

    #[test]
    fn exact_name_lookup_returns_duplicate_names_in_id_order() {
        let mut db = index::open(Path::new(":memory:")).unwrap();
        let mut first = fixture_skill("duplicate", "first");
        first.path = "/first/SKILL.md".into();
        first.canonical = "/first/SKILL.md".into();
        first.base = "/first".into();
        let mut second = fixture_skill("duplicate", "second");
        second.path = "/second/SKILL.md".into();
        second.canonical = "/second/SKILL.md".into();
        second.base = "/second".into();
        index::refresh_kind(&mut db, "filesystem", &[first, second], true).unwrap();

        let rows = find_name(&db, None, "duplicate").unwrap();
        assert_eq!(rows.len(), 2);
        let ids: Vec<_> = rows.iter().map(|row| row.id.as_str()).collect();
        let mut sorted_ids = ids.clone();
        sorted_ids.sort_unstable();
        assert_eq!(ids, sorted_ids);
        assert_eq!(
            ids,
            find_name(&db, None, "duplicate")
                .unwrap()
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn exact_name_lookup_excludes_disabled_and_denied_records() {
        let mut db = index::open(Path::new(":memory:")).unwrap();
        let mut disabled = fixture_skill("disabled", "disabled");
        disabled.enabled = false;
        let mut denied = fixture_skill("denied", "denied");
        denied.metadata.invocation_policy = crate::metadata::InvocationPolicy::Denied;
        let visible = fixture_skill("visible", "visible");
        index::refresh_kind(&mut db, "filesystem", &[disabled, denied, visible], true).unwrap();

        assert!(find_name(&db, None, "disabled").unwrap().is_empty());
        assert!(find_name(&db, None, "denied").unwrap().is_empty());
        assert_eq!(find_name(&db, None, "visible").unwrap().len(), 1);
    }

    #[test]
    fn public_inventory_hides_disabled_records() {
        let mut db = index::open(std::path::Path::new(":memory:")).unwrap();
        let mut skill = Skill {
            path: "/hidden/SKILL.md".into(),
            canonical: "/hidden/SKILL.md".into(),
            base: "/hidden".into(),
            workspace: None,
            codex_home: None,
            scope: "global".into(),
            source: "/hidden".into(),
            source_kind: "filesystem".into(),
            enabled: false,
            plugin_id: None,
            metadata: Metadata {
                name: "hidden".into(),
                description: "manual only".into(),
                keywords: "manual".into(),
                degraded: false,
                hash: "hash".into(),
                invocation_policy: crate::metadata::InvocationPolicy::Discoverable,
                policy_diagnostic: None,
            },
        };
        index::refresh_kind(&mut db, "filesystem", std::slice::from_ref(&skill), true).unwrap();
        let id: String = db
            .query_row("SELECT id FROM skills", [], |row| row.get(0))
            .unwrap();
        assert!(find(&db, None, &id).unwrap().is_none());
        assert!(all(&db, None, None).unwrap().is_empty());
        assert!(query(&db, None, "manual", 5).unwrap().is_empty());
        assert_eq!(count(&db, None).unwrap(), 0);

        skill.enabled = true;
        index::refresh_kind(&mut db, "filesystem", &[skill], true).unwrap();
        assert_eq!(count(&db, None).unwrap(), 1);
    }

    #[test]
    fn public_inventory_uses_only_the_requested_native_context() {
        let workspace_a = tempfile::tempdir().unwrap();
        let workspace_b = tempfile::tempdir().unwrap();
        let home_a = tempfile::tempdir().unwrap();
        let home_b = tempfile::tempdir().unwrap();
        let context_a = Context {
            workspace: std::fs::canonicalize(workspace_a.path()).unwrap(),
            codex_home: std::fs::canonicalize(home_a.path()).unwrap(),
        };
        let context_b = Context {
            workspace: std::fs::canonicalize(workspace_b.path()).unwrap(),
            codex_home: std::fs::canonicalize(home_b.path()).unwrap(),
        };
        let mut db = index::open(Path::new(":memory:")).unwrap();
        let mut filesystem = fixture_skill("shared", "filesystem shared");
        filesystem.path = "/shared/SKILL.md".into();
        filesystem.canonical = "/shared/SKILL.md".into();
        filesystem.base = "/shared".into();
        index::refresh_kind(&mut db, "filesystem", &[filesystem], true).unwrap();
        let mut native_a = fixture_skill("native-a", "native A");
        native_a.path = "/native-a/SKILL.md".into();
        native_a.canonical = "/native-a/SKILL.md".into();
        native_a.base = "/native-a".into();
        native_a.source_kind = "codex".into();
        native_a.workspace = Some(context_a.workspace.clone());
        native_a.codex_home = Some(context_a.codex_home.clone());
        let mut native_b = native_a.clone();
        native_b.metadata.name = "native-b".into();
        native_b.metadata.description = "native B".into();
        native_b.path = "/native-b/SKILL.md".into();
        native_b.canonical = "/native-b/SKILL.md".into();
        native_b.base = "/native-b".into();
        native_b.workspace = Some(context_b.workspace.clone());
        native_b.codex_home = Some(context_b.codex_home.clone());
        index::refresh_kind_for_context(
            &mut db,
            "codex",
            Some((&context_a.workspace, &context_a.codex_home)),
            &[native_a],
            true,
        )
        .unwrap();
        index::refresh_kind_for_context(
            &mut db,
            "codex",
            Some((&context_b.workspace, &context_b.codex_home)),
            &[native_b],
            true,
        )
        .unwrap();

        let names_a: Vec<_> = all(&db, Some(&context_a), None)
            .unwrap()
            .into_iter()
            .map(|row| row.name)
            .collect();
        let names_b: Vec<_> = all(&db, Some(&context_b), None)
            .unwrap()
            .into_iter()
            .map(|row| row.name)
            .collect();
        assert_eq!(names_a, vec!["native-a", "shared"]);
        assert_eq!(names_b, vec!["native-b", "shared"]);
        assert_eq!(count(&db, Some(&context_a)).unwrap(), 2);
        assert_eq!(count(&db, Some(&context_b)).unwrap(), 2);
        assert_eq!(
            find_name(&db, Some(&context_a), "native-a")
                .unwrap()
                .iter()
                .map(|row| row.name.as_str())
                .collect::<Vec<_>>(),
            vec!["native-a"]
        );
        assert!(find_name(&db, Some(&context_a), "native-b")
            .unwrap()
            .is_empty());
        assert_eq!(
            find_name(&db, Some(&context_b), "native-b")
                .unwrap()
                .iter()
                .map(|row| row.name.as_str())
                .collect::<Vec<_>>(),
            vec!["native-b"]
        );
        assert!(find_name(&db, Some(&context_b), "native-a")
            .unwrap()
            .is_empty());
        assert!(find(&db, Some(&context_a), "native-b@missing")
            .unwrap()
            .is_none());
    }

    #[test]
    fn exact_name_lookup_partitions_workspace_and_codex_home_independently() {
        let context = |workspace: &str, codex_home: &str| Context {
            workspace: PathBuf::from(workspace),
            codex_home: PathBuf::from(codex_home),
        };
        let same_workspace_home_a = context("/shared-workspace", "/home-a");
        let same_workspace_home_b = context("/shared-workspace", "/home-b");
        let workspace_a_shared_home = context("/workspace-a", "/shared-home");
        let workspace_b_shared_home = context("/workspace-b", "/shared-home");
        let mut db = index::open(Path::new(":memory:")).unwrap();

        let native = |name: &str, context: &Context| {
            let mut skill = fixture_skill(name, name);
            skill.source_kind = "codex".into();
            skill.workspace = Some(context.workspace.clone());
            skill.codex_home = Some(context.codex_home.clone());
            skill
        };
        let records = [
            ("same-workspace-home-a", &same_workspace_home_a),
            ("same-workspace-home-b", &same_workspace_home_b),
            ("workspace-a-shared-home", &workspace_a_shared_home),
            ("workspace-b-shared-home", &workspace_b_shared_home),
        ];
        for (name, context) in records {
            let skill = native(name, context);
            index::refresh_kind_for_context(
                &mut db,
                "codex",
                Some((&context.workspace, &context.codex_home)),
                &[skill],
                true,
            )
            .unwrap();
        }

        let names = |context: &Context, name: &str| {
            find_name(&db, Some(context), name)
                .unwrap()
                .into_iter()
                .map(|row| row.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(&same_workspace_home_a, "same-workspace-home-a"),
            vec!["same-workspace-home-a"]
        );
        assert!(names(&same_workspace_home_b, "same-workspace-home-a").is_empty());
        assert!(names(&same_workspace_home_a, "same-workspace-home-b").is_empty());
        assert_eq!(
            names(&same_workspace_home_b, "same-workspace-home-b"),
            vec!["same-workspace-home-b"]
        );
        assert_eq!(
            names(&workspace_a_shared_home, "workspace-a-shared-home"),
            vec!["workspace-a-shared-home"]
        );
        assert!(names(&workspace_b_shared_home, "workspace-a-shared-home").is_empty());
        assert!(names(&workspace_a_shared_home, "workspace-b-shared-home").is_empty());
        assert_eq!(
            names(&workspace_b_shared_home, "workspace-b-shared-home"),
            vec!["workspace-b-shared-home"]
        );
    }
}
