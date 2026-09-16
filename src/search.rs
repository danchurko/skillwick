use rusqlite::{params_from_iter, types::Value, Connection};
use serde::Serialize;
use std::{cmp::Ordering, collections::HashMap, path::Path};

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

pub fn query(
    db: &Connection,
    query: &str,
    limit: usize,
    roots: Option<&[String]>,
) -> rusqlite::Result<Vec<ResultRow>> {
    let query_tokens = tokens(query);
    if query_tokens.is_empty() {
        return Ok(Vec::new());
    }
    let mut sql = "SELECT s.id,s.name,s.description,s.scope,s.path,s.canonical,s.base,s.source,s.source_kind,s.enabled,s.plugin_id,s.degraded,s.hash,bm25(skills_fts,8.0,3.0,1.0),lower(s.name)||' '||lower(s.description)||' '||lower(s.keywords) FROM skills_fts JOIN skills s ON s.id=skills_fts.id WHERE skills_fts MATCH ?1 AND s.enabled=1 AND s.model_discoverable=1 AND s.source_kind='filesystem'".to_owned();
    sql.push_str(&root_filter("s", roots, 2));
    let mut values = vec![Value::Text(expression(&query_tokens))];
    append_root_values(&mut values, roots);
    let mut statement = db.prepare(&sql)?;
    let mapped = statement.query_map(params_from_iter(values), |row| {
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
        })
    })?;
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
