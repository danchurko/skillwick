use rusqlite::Connection;
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

pub fn query(db: &Connection, query: &str, limit: usize) -> rusqlite::Result<Vec<ResultRow>> {
    let query_tokens = tokens(query);
    if query_tokens.is_empty() {
        return Ok(Vec::new());
    }
    let mut statement = db.prepare("SELECT s.id,s.name,s.description,s.scope,s.path,s.canonical,s.base,s.source,s.source_kind,s.enabled,s.plugin_id,s.degraded,s.hash,bm25(skills_fts,8.0,3.0,1.0),lower(s.name)||' '||lower(s.description)||' '||lower(s.keywords) FROM skills_fts JOIN skills s ON s.id=skills_fts.id WHERE skills_fts MATCH ?1 AND s.enabled=1 AND NOT (s.source_kind='filesystem' AND EXISTS (SELECT 1 FROM skills n WHERE n.source_kind='codex' AND n.canonical=s.canonical))")?;
    let mapped = statement.query_map([expression(&query_tokens)], |row| {
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
    Ok(candidates
        .into_iter()
        .take(limit)
        .map(|candidate| {
            let _ = candidate.search_text;
            candidate.row
        })
        .collect())
}

pub fn all(db: &Connection, limit: Option<usize>) -> rusqlite::Result<Vec<ResultRow>> {
    let sql = if limit.is_some() {
        "SELECT s.id,s.name,s.description,s.scope,s.path,s.canonical,s.base,s.source,s.source_kind,s.enabled,s.plugin_id,s.degraded,s.hash FROM skills s WHERE NOT (s.source_kind='filesystem' AND EXISTS (SELECT 1 FROM skills n WHERE n.source_kind='codex' AND n.canonical=s.canonical)) ORDER BY lower(s.name),s.id LIMIT ?1"
    } else {
        "SELECT s.id,s.name,s.description,s.scope,s.path,s.canonical,s.base,s.source,s.source_kind,s.enabled,s.plugin_id,s.degraded,s.hash FROM skills s WHERE NOT (s.source_kind='filesystem' AND EXISTS (SELECT 1 FROM skills n WHERE n.source_kind='codex' AND n.canonical=s.canonical)) ORDER BY lower(s.name),s.id"
    };
    let mut statement = db.prepare(sql)?;
    if let Some(limit) = limit {
        statement.query_map([limit as i64], row_from)?.collect()
    } else {
        statement.query_map([], row_from)?.collect()
    }
}

pub fn find(db: &Connection, id: &str) -> rusqlite::Result<Option<ResultRow>> {
    let mut statement = db.prepare("SELECT id,name,description,scope,path,canonical,base,source,source_kind,enabled,plugin_id,degraded,hash FROM skills WHERE id=?1")?;
    let mut rows = statement.query([id])?;
    rows.next()?.map(row_from).transpose()
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
    #[test]
    fn ranks_exact_and_handles_technical_tokens_stably() {
        let mut db = index::open(Path::new(":memory:")).unwrap();
        let make = |name: &str, description: &str| Skill {
            path: PathBuf::from(format!("/{name}/SKILL.md")),
            canonical: PathBuf::from(format!("/{name}/SKILL.md")),
            base: PathBuf::from(format!("/{name}")),
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
        let first = query(&db, "C++", 5).unwrap();
        let second = query(&db, "C++", 5).unwrap();
        assert_eq!(first[0].name, "C++");
        assert_eq!(
            first.iter().map(|row| &row.id).collect::<Vec<_>>(),
            second.iter().map(|row| &row.id).collect::<Vec<_>>()
        );
        assert!(query(&db, "quoted \" text", 5).is_ok());
    }
}
