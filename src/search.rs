use rusqlite::{params_from_iter, types::Value, Connection};
use serde::Serialize;
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    path::Path,
};

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
    pub origins: Vec<Origin>,
    pub grouping_diagnostic: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Origin {
    pub id: String,
    pub path: String,
    pub canonical: String,
    pub base: String,
    pub source: String,
    pub scope: String,
    pub plugin_id: Option<String>,
}

impl Origin {
    fn from_row(row: &ResultRow) -> Self {
        Self {
            id: row.id.clone(),
            path: row.path.clone(),
            canonical: row.canonical.clone(),
            base: row.base.clone(),
            source: row.source.clone(),
            scope: row.scope.clone(),
            plugin_id: row.plugin_id.clone(),
        }
    }
}

/// Group only complete verified copies, retaining rank order and every origin.
fn group(rows: Vec<ResultRow>) -> Vec<ResultRow> {
    let mut counts = HashMap::new();
    for row in &rows {
        *counts.entry(row.hash.clone()).or_insert(0usize) += 1;
    }
    let mut positions: HashMap<String, usize> = HashMap::new();
    let mut result: Vec<ResultRow> = Vec::new();
    for mut row in rows {
        if row.origins.is_empty() {
            row.origins.push(Origin::from_row(&row));
        }
        let key = if counts[&row.hash] > 1 {
            match crate::package::fingerprint(Path::new(&row.base)) {
                Ok(fingerprint) => format!("{}:{fingerprint}", row.hash),
                Err(error) => {
                    row.grouping_diagnostic = Some(error);
                    row.id.clone()
                }
            }
        } else {
            row.id.clone()
        };
        if let Some(&position) = positions.get(&key) {
            let previous = &mut result[position];
            let mut origins = std::mem::take(&mut previous.origins);
            origins.append(&mut row.origins);
            origins.sort_by(|a, b| a.canonical.cmp(&b.canonical).then(a.source.cmp(&b.source)));
            if row.canonical < previous.canonical {
                *previous = row;
            }
            previous.origins = origins;
        } else {
            positions.insert(key, result.len());
            result.push(row);
        }
    }
    result
}

struct Candidate {
    row: ResultRow,
    score: f64,
    exact: bool,
    coverage: usize,
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

fn term_coverage(
    db: &Connection,
    query_tokens: &[String],
) -> rusqlite::Result<HashMap<String, usize>> {
    let mut statement = db.prepare("SELECT id FROM skills_fts WHERE skills_fts MATCH ?1")?;
    let mut coverage = HashMap::new();
    for token in query_tokens {
        let term = expression(std::slice::from_ref(token));
        let matching_ids = statement.query_map([term], |row| row.get::<_, String>(0))?;
        for id in matching_ids {
            *coverage.entry(id?).or_insert(0) += 1;
        }
    }
    Ok(coverage)
}

fn name_matches(db: &Connection, expression: &str) -> rusqlite::Result<HashSet<String>> {
    let mut statement = db.prepare("SELECT id FROM skills_fts WHERE name MATCH ?1")?;
    let mut matches = HashSet::new();
    let matching_ids = statement.query_map([expression], |row| row.get::<_, String>(0))?;
    for id in matching_ids {
        matches.insert(id?);
    }
    Ok(matches)
}

pub fn query(
    db: &Connection,
    query: &str,
    limit: usize,
    roots: Option<&[String]>,
) -> rusqlite::Result<Vec<ResultRow>> {
    let mut seen_tokens = HashSet::new();
    let query_tokens = tokens(query)
        .into_iter()
        .filter(|token| seen_tokens.insert(token.clone()))
        .collect::<Vec<_>>();
    if query_tokens.is_empty() {
        return Ok(Vec::new());
    }
    let expression = expression(&query_tokens);
    let mapped = {
        let mut sql = "SELECT s.id,s.name,s.description,s.scope,s.path,s.canonical,s.base,s.source,s.source_kind,s.enabled,s.plugin_id,s.degraded,s.hash,bm25(skills_fts,0.0,8.0,3.0,1.0) FROM skills_fts JOIN skills s ON s.id=skills_fts.id WHERE skills_fts MATCH ?1 AND s.enabled=1 AND s.model_discoverable=1 AND s.source_kind='filesystem'".to_owned();
        sql.push_str(&root_filter("s", roots, 2));
        let mut values = vec![Value::Text(expression.clone())];
        append_root_values(&mut values, roots);
        let mut statement = db.prepare(&sql)?;
        let candidates = statement
            .query_map(params_from_iter(values), |row| {
                let result = row_from(row)?;
                Ok(Candidate {
                    exact: result.name.to_lowercase() == query.trim().to_lowercase(),
                    score: row.get(13)?,
                    row: result,
                    coverage: 0,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        candidates
    };
    let coverage_by_id = term_coverage(db, &query_tokens)?;
    let name_matches = name_matches(db, &expression)?;
    let minimum_coverage = if query_tokens.len() >= 3 { 2 } else { 1 };
    let mut candidates: Vec<_> = mapped
        .into_iter()
        .map(|mut candidate| {
            candidate.coverage = coverage_by_id
                .get(&candidate.row.id)
                .copied()
                .unwrap_or_default();
            candidate
        })
        .filter(|candidate| {
            candidate.exact
                || candidate.coverage >= minimum_coverage
                || name_matches.contains(&candidate.row.id)
        })
        .collect();
    candidates.sort_by(|left, right| {
        right
            .exact
            .cmp(&left.exact)
            .then_with(|| {
                left.score
                    .partial_cmp(&right.score)
                    .unwrap_or(Ordering::Equal)
            })
            .then(right.coverage.cmp(&left.coverage))
            .then_with(|| {
                left.row
                    .name
                    .to_lowercase()
                    .cmp(&right.row.name.to_lowercase())
            })
            .then(left.row.id.cmp(&right.row.id))
    });
    let mut rows = candidates
        .into_iter()
        .map(|candidate| candidate.row)
        .collect::<Vec<_>>();
    contextualize(db, &mut rows, roots)?;
    Ok(group(rows).into_iter().take(limit).collect())
}

pub fn all(
    db: &Connection,
    limit: Option<usize>,
    roots: Option<&[String]>,
) -> rusqlite::Result<Vec<ResultRow>> {
    let mut sql = "SELECT s.id,s.name,s.description,s.scope,s.path,s.canonical,s.base,s.source,s.source_kind,s.enabled,s.plugin_id,s.degraded,s.hash FROM skills s WHERE s.enabled=1 AND s.model_discoverable=1 AND s.source_kind='filesystem'".to_owned();
    sql.push_str(&root_filter("s", roots, 1));
    sql.push_str(" ORDER BY lower(s.name),s.id");
    let mut values = Vec::new();
    append_root_values(&mut values, roots);
    let mut statement = db.prepare(&sql)?;
    let mut rows = statement
        .query_map(params_from_iter(values), row_from)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    contextualize(db, &mut rows, roots)?;
    Ok(group(rows)
        .into_iter()
        .take(limit.unwrap_or(usize::MAX))
        .collect())
}

pub fn count(db: &Connection, roots: Option<&[String]>) -> rusqlite::Result<usize> {
    Ok(all(db, None, roots)?.len())
}

pub fn find(
    db: &Connection,
    id: &str,
    roots: Option<&[String]>,
) -> rusqlite::Result<Option<ResultRow>> {
    let mut sql = "SELECT s.id,s.name,s.description,s.scope,s.path,s.canonical,s.base,s.source,s.source_kind,s.enabled,s.plugin_id,s.degraded,s.hash FROM skills s WHERE s.id=?1 AND s.enabled=1 AND s.model_discoverable=1 AND s.source_kind='filesystem'".to_owned();
    sql.push_str(&root_filter("s", roots, 2));
    let mut values = vec![Value::Text(id.to_owned())];
    append_root_values(&mut values, roots);
    let mut statement = db.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(values))?;
    let mut result = rows.next()?.map(row_from).transpose()?;
    if let Some(row) = &mut result {
        contextualize(db, std::slice::from_mut(row), roots)?;
        for group in find_name(db, &row.name, roots)? {
            if group.origins.iter().any(|origin| origin.id == row.id) {
                row.origins = group.origins;
                row.grouping_diagnostic = group.grouping_diagnostic;
                break;
            }
        }
    }
    Ok(result)
}

pub fn find_name(
    db: &Connection,
    name: &str,
    roots: Option<&[String]>,
) -> rusqlite::Result<Vec<ResultRow>> {
    let mut sql = "SELECT s.id,s.name,s.description,s.scope,s.path,s.canonical,s.base,s.source,s.source_kind,s.enabled,s.plugin_id,s.degraded,s.hash FROM skills s WHERE s.name=?1 AND s.enabled=1 AND s.model_discoverable=1 AND s.source_kind='filesystem'".to_owned();
    sql.push_str(&root_filter("s", roots, 2));
    sql.push_str(" ORDER BY s.id");
    let mut values = vec![Value::Text(name.to_owned())];
    append_root_values(&mut values, roots);
    let mut statement = db.prepare(&sql)?;
    let mut rows = statement
        .query_map(params_from_iter(values), row_from)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    contextualize(db, &mut rows, roots)?;
    Ok(group(rows))
}

/// Diagnose a denied exact selection only within the current applicable roots.
pub fn policy_denied(
    db: &Connection,
    target: &str,
    roots: Option<&[String]>,
) -> rusqlite::Result<bool> {
    let mut sql = "SELECT EXISTS(SELECT 1 FROM skills s WHERE (s.id=?1 OR s.name=?1) AND s.model_discoverable=0 AND s.source_kind='filesystem'".to_owned();
    sql.push_str(&root_filter("s", roots, 2));
    sql.push(')');
    let mut values = vec![Value::Text(target.to_owned())];
    append_root_values(&mut values, roots);
    db.query_row(&sql, params_from_iter(values), |row| row.get(0))
}

fn contextualize(
    db: &Connection,
    rows: &mut [ResultRow],
    roots: Option<&[String]>,
) -> rusqlite::Result<()> {
    for row in rows {
        let mut sql = "SELECT scope,root FROM skill_roots WHERE skill_id=?1".to_owned();
        let mut values = vec![Value::Text(row.id.clone())];
        if let Some(roots) = roots {
            let placeholders = (0..roots.len())
                .map(|i| format!("?{}", i + 2))
                .collect::<Vec<_>>()
                .join(",");
            sql.push_str(&format!(" AND root IN ({placeholders})"));
            values.extend(roots.iter().cloned().map(Value::Text));
        }
        sql.push_str(" ORDER BY CASE scope WHEN 'global' THEN 0 ELSE 1 END,root");
        let mut statement = db.prepare(&sql)?;
        let associations = statement
            .query_map(params_from_iter(values), |record| {
                Ok((record.get::<_, String>(0)?, record.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (scope, root) in associations {
            let mut origin = Origin::from_row(row);
            origin.scope = scope;
            origin.source = root;
            row.origins.push(origin);
        }
        if let Some(origin) = row.origins.first() {
            row.scope = origin.scope.clone();
            row.source = origin.source.clone();
        } else {
            row.origins.push(Origin::from_row(row));
        }
    }
    Ok(())
}

pub(crate) fn root_filter(alias: &str, roots: Option<&[String]>, first_parameter: usize) -> String {
    let Some(roots) = roots else {
        return String::new();
    };
    if roots.is_empty() {
        return " AND 0".into();
    }
    let placeholders = (0..roots.len())
        .map(|offset| format!("?{}", first_parameter + offset))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        " AND EXISTS (SELECT 1 FROM skill_roots sr WHERE sr.skill_id={alias}.id AND sr.root IN ({placeholders}))"
    )
}

pub(crate) fn append_root_values(values: &mut Vec<Value>, roots: Option<&[String]>) {
    if let Some(roots) = roots {
        values.extend(roots.iter().cloned().map(Value::Text));
    }
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
        origins: Vec::new(),
        grouping_diagnostic: None,
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

    #[test]
    fn copies_group_only_when_complete_packages_match() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        for name in ["a", "b", "c"] {
            let package = root.join(name);
            std::fs::create_dir(&package).unwrap();
            std::fs::write(
                package.join("SKILL.md"),
                "---\nname: copied\ndescription: deploy cobalt runtime\n---\nRead helper.txt.\n",
            )
            .unwrap();
            std::fs::write(
                package.join("helper.txt"),
                if name == "c" { "different" } else { "same" },
            )
            .unwrap();
        }
        let scan = crate::sources::scan(
            root,
            &crate::discovery::Report {
                sources: vec![crate::discovery::SourceSpec {
                    provider: "custom".into(),
                    scope: "global".into(),
                    root: root.to_str().unwrap().into(),
                    plugin_id: None,
                    version: None,
                    provenance: "fixture".into(),
                }],
                configured_sources: Vec::new(),
                configured_roots: Vec::new(),
                diagnostics: Vec::new(),
                complete: true,
            },
        );
        assert!(scan.complete);
        let mut db = index::open(Path::new(":memory:")).unwrap();
        index::refresh_kind(&mut db, "filesystem", &scan.skills, true).unwrap();
        let rows = query(&db, "deploy cobalt", 2, None).unwrap();
        assert_eq!(rows.len(), 2);
        let copies = rows.iter().find(|row| row.origins.len() == 2).unwrap();
        assert!(copies.canonical.ends_with("a/SKILL.md"));
        for origin in &copies.origins {
            assert_eq!(
                find(&db, &origin.id, None).unwrap().unwrap().canonical,
                origin.canonical
            );
        }
        assert_eq!(find_name(&db, "copied", None).unwrap().len(), 2);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("helper.txt", root.join("b/link")).unwrap();
            assert_eq!(find_name(&db, "copied", None).unwrap().len(), 3);
        }
    }

    fn fixture_skill(name: &str, description: &str) -> Skill {
        Skill {
            path: PathBuf::from(format!("/{name}/SKILL.md")),
            canonical: PathBuf::from(format!("/{name}/SKILL.md")),
            base: PathBuf::from(format!("/{name}")),
            scope: "global".into(),
            source: format!("/{name}"),
            source_kind: "filesystem".into(),
            enabled: true,
            plugin_id: None,
            source_fingerprint: "fingerprint".into(),
            roots: Vec::new(),
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
        let make = |name: &str, description: &str| fixture_skill(name, description);
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
        let first = query(&db, "C++", 5, None).unwrap();
        let second = query(&db, "C++", 5, None).unwrap();
        assert_eq!(first[0].name, "C++");
        assert_eq!(
            first.iter().map(|row| &row.id).collect::<Vec<_>>(),
            second.iter().map(|row| &row.id).collect::<Vec<_>>()
        );
        assert!(query(&db, "quoted \" text", 5, None).is_ok());
        assert_eq!(count(&db, None).unwrap(), 2);
    }

    #[test]
    fn uses_native_term_coverage_and_name_weighted_bm25() {
        let mut db = index::open(Path::new(":memory:")).unwrap();
        let long_metadata = format!(
            "{} aws dynamodb microservices",
            "catalog guidance ".repeat(128)
        );
        index::refresh_kind(
            &mut db,
            "filesystem",
            &[
                fixture_skill("aws-dynamodb", "connection"),
                fixture_skill("catalog", &long_metadata),
            ],
            true,
        )
        .unwrap();

        let rows = query(&db, "aws dynamodb service", 5, None).unwrap();

        assert_eq!(rows[0].name, "aws-dynamodb");
        assert!(rows.iter().any(|row| row.name == "catalog"));
    }

    #[test]
    fn substring_and_duplicate_terms_do_not_inflate_coverage() {
        let mut db = index::open(Path::new(":memory:")).unwrap();
        index::refresh_kind(
            &mut db,
            "filesystem",
            &[
                fixture_skill("search-guide", "retrieval inside"),
                fixture_skill("database-guide", "dynamodb connector"),
            ],
            true,
        )
        .unwrap();

        assert!(query(&db, "retrieval is runtime", 5, None)
            .unwrap()
            .is_empty());
        assert!(query(&db, "dynamodb dynamodb service runtime", 5, None)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn native_name_anchor_admits_rare_names_without_admitting_phantoms_or_noise() {
        let mut db = index::open(Path::new(":memory:")).unwrap();
        index::refresh_kind(
            &mut db,
            "filesystem",
            &[
                fixture_skill("acme-cobalt", "Package connector"),
                fixture_skill("assistant", "AWS"),
                fixture_skill("cobaltic", "AWS"),
            ],
            true,
        )
        .unwrap();

        let rows = query(
            &db,
            "Cobalt AWS direct inference TypeScript tool use evaluation",
            5,
            None,
        )
        .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "acme-cobalt");
    }

    #[test]
    fn technical_aliases_and_unicode_still_match() {
        let mut db = index::open(Path::new(":memory:")).unwrap();
        index::refresh_kind(
            &mut db,
            "filesystem",
            &[
                fixture_skill("C++", "Native tools"),
                fixture_skill("C#", "Managed tools"),
                fixture_skill(".NET", "Web tools"),
                fixture_skill("node.js", "JavaScript tools"),
                fixture_skill("café", "Unicode tools"),
            ],
            true,
        )
        .unwrap();

        for name in ["C++", "C#", ".NET", "node.js", "café"] {
            assert_eq!(query(&db, name, 5, None).unwrap()[0].name, name);
        }
    }

    #[test]
    fn keeps_query_eligibility_root_and_scope_filters() {
        let mut db = index::open(Path::new(":memory:")).unwrap();
        let project_root = PathBuf::from("/roots/project");
        let global_root = PathBuf::from("/roots/global");
        let mut eligible = fixture_skill("acme-cobalt", "Package connector");
        eligible.roots = vec![(project_root.clone(), "project".into())];
        let mut other_root = fixture_skill("global-cobalt", "Package connector");
        other_root.roots = vec![(global_root.clone(), "global".into())];
        let mut disabled = fixture_skill("disabled-cobalt", "Package connector");
        disabled.enabled = false;
        disabled.roots = vec![(project_root.clone(), "project".into())];
        let mut denied = fixture_skill("denied-cobalt", "Package connector");
        denied.metadata.invocation_policy = crate::metadata::InvocationPolicy::Denied;
        denied.roots = vec![(project_root.clone(), "project".into())];
        index::refresh_filesystem_scope(
            &mut db,
            &[eligible, other_root, disabled, denied],
            &[
                (project_root.clone(), "project".into()),
                (global_root, "global".into()),
            ],
            true,
        )
        .unwrap();

        let roots = [project_root.to_string_lossy().into_owned()];
        let rows = query(
            &db,
            "Cobalt AWS direct inference TypeScript tool use evaluation",
            5,
            Some(&roots),
        )
        .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "acme-cobalt");
        assert_eq!(rows[0].scope, "project");
    }

    #[test]
    fn exact_name_lookup_is_case_sensitive_and_stable() {
        let mut db = index::open(Path::new(":memory:")).unwrap();
        let skill = fixture_skill("Readable Name", "An exact-name fixture");
        index::refresh_kind(&mut db, "filesystem", &[skill], true).unwrap();
        assert_eq!(find_name(&db, "Readable Name", None).unwrap().len(), 1);
        assert!(find_name(&db, "readable name", None).unwrap().is_empty());
    }

    #[test]
    fn public_inventory_hides_disabled_and_denied_records() {
        let mut db = index::open(Path::new(":memory:")).unwrap();
        let mut disabled = fixture_skill("disabled", "disabled");
        disabled.enabled = false;
        let mut denied = fixture_skill("denied", "denied");
        denied.metadata.invocation_policy = crate::metadata::InvocationPolicy::Denied;
        let visible = fixture_skill("visible", "visible");
        index::refresh_kind(&mut db, "filesystem", &[disabled, denied, visible], true).unwrap();
        assert_eq!(count(&db, None).unwrap(), 1);
        assert!(find_name(&db, "disabled", None).unwrap().is_empty());
        assert!(find_name(&db, "denied", None).unwrap().is_empty());
        assert_eq!(find_name(&db, "visible", None).unwrap().len(), 1);
    }
}
