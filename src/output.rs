use crate::search::ResultRow;
use serde::Serialize;
use std::io::{self, Write};

const MAX_TEXT_BYTES: usize = 2_000;

pub fn text(rows: &[ResultRow]) -> io::Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    if rows.is_empty() {
        return writeln!(output, "No matching skills.");
    }
    let mut used = 0;
    for row in rows {
        let description = clean(row.description.lines().next().unwrap_or(""));
        let scope = if let Some(plugin) = &row.plugin_id {
            format!("plugin:{}", clean(plugin))
        } else {
            clean(&row.scope)
        };
        let line = format!("{} [{}] {}\n", clean(&row.id), scope, description);
        if used + line.len() > MAX_TEXT_BYTES {
            break;
        }
        output.write_all(line.as_bytes())?;
        used += line.len();
    }
    Ok(())
}

pub fn json(rows: &[ResultRow]) -> io::Result<()> {
    let rows: Vec<_> = rows.iter().cloned().map(clean_row).collect();
    #[derive(Serialize)]
    struct Envelope<'a> {
        version: u8,
        results: &'a [ResultRow],
    }
    let stdout = io::stdout();
    writeln!(
        stdout.lock(),
        "{}",
        serde_json::to_string(&Envelope {
            version: 1,
            results: &rows
        })
        .expect("serializable output")
    )
}

pub fn list_text(rows: &[ResultRow], total: usize, all: bool) -> io::Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(output, "{total} skills in the current inventory.")?;
    if total == 0 {
        return Ok(());
    }
    if !all && rows.len() < total {
        writeln!(
            output,
            "Showing up to {} records; use `skillwick list --all` for every record.",
            rows.len()
        )?;
    }
    for row in rows {
        writeln!(output, "{}", list_line(row))?;
    }
    Ok(())
}

fn list_line(row: &ResultRow) -> String {
    let scope = if let Some(plugin) = &row.plugin_id {
        format!("plugin:{}", clean(plugin))
    } else {
        clean(&row.scope)
    };
    format!(
        "{} [{}]{}",
        clean(&row.id),
        scope,
        if row.enabled { "" } else { " (disabled)" }
    )
}

pub fn list_json(rows: &[ResultRow], total: usize) -> io::Result<()> {
    let rows: Vec<_> = rows.iter().cloned().map(clean_row).collect();
    #[derive(Serialize)]
    struct Envelope<'a> {
        version: u8,
        total: usize,
        results: &'a [ResultRow],
    }
    writeln!(
        io::stdout().lock(),
        "{}",
        serde_json::to_string(&Envelope {
            version: 1,
            total,
            results: &rows,
        })
        .expect("serializable output")
    )
}

fn clean_row(mut row: ResultRow) -> ResultRow {
    row.id = clean(&row.id);
    row.name = clean(&row.name);
    row.description = clean(&row.description);
    row.scope = clean(&row.scope);
    row.path = clean(&row.path);
    row.canonical = clean(&row.canonical);
    row.base = clean(&row.base);
    row.source = clean(&row.source);
    row.source_kind = clean(&row.source_kind);
    row.plugin_id = row.plugin_id.map(|value| clean(&value));
    row.hash = clean(&row.hash);
    row
}

pub fn clean(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
            continue;
        }
        if !character.is_control() || character == '\t' {
            result.push(character);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_terminal_controls() {
        assert_eq!(clean("safe\u{1b}[31m red\n"), "safe red");
    }

    #[test]
    fn sanitizes_every_json_string_field() {
        let unsafe_text = "safe\u{1b}[31m\n".to_string();
        let cleaned = clean_row(ResultRow {
            id: unsafe_text.clone(),
            name: unsafe_text.clone(),
            description: unsafe_text.clone(),
            scope: unsafe_text.clone(),
            path: unsafe_text.clone(),
            canonical: unsafe_text.clone(),
            base: unsafe_text.clone(),
            source: unsafe_text.clone(),
            source_kind: unsafe_text.clone(),
            enabled: true,
            plugin_id: Some(unsafe_text.clone()),
            degraded: false,
            hash: unsafe_text,
        });
        let serialized = serde_json::to_string(&cleaned).unwrap();
        assert!(!serialized.contains("\\u001b"));
        assert!(!serialized.contains("\\n"));
        let mut disabled = cleaned;
        disabled.enabled = false;
        assert!(list_line(&disabled).ends_with(" (disabled)"));
    }
}
