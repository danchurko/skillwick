use crate::search::ResultRow;
use serde::Serialize;
use std::io::{self, Write};

const MAX_RECORD_BYTES: usize = 2_000;
const TRUNCATION_MARKER: &str = " [truncated]";
pub const JSON_VERSION: u8 = 2;

pub fn text(rows: &[ResultRow]) -> io::Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    if rows.is_empty() {
        return writeln!(output, "No matching skills.");
    }
    for row in rows {
        let line = bounded_line(row, MAX_RECORD_BYTES);
        output.write_all(line.as_bytes())?;
    }
    Ok(())
}

fn bounded_line(row: &ResultRow, budget: usize) -> String {
    let id = clean(&row.id);
    let scope = if let Some(plugin) = &row.plugin_id {
        format!("plugin:{}", clean(plugin))
    } else {
        clean(&row.scope)
    };
    let description = clean(row.description.lines().next().unwrap_or(""));
    let prefix = format!("{id} [{scope}] ");
    let newline = "\n";
    let line = format!("{prefix}{description}{newline}");

    if line.len() <= budget {
        return line;
    }

    // Keep every candidate visible while bounding each record independently.
    if budget == 0 {
        return String::new();
    }
    let content_budget = budget.saturating_sub(newline.len() + TRUNCATION_MARKER.len());
    if content_budget == 0 {
        return format!(
            "{}\n",
            truncate_utf8(TRUNCATION_MARKER, budget.saturating_sub(1))
        );
    }
    if prefix.len() < content_budget {
        let description_budget = content_budget - prefix.len();
        return format!(
            "{prefix}{}{TRUNCATION_MARKER}{newline}",
            truncate_utf8(&description, description_budget)
        );
    }

    format!(
        "{}{TRUNCATION_MARKER}{newline}",
        truncate_utf8(&prefix, content_budget)
    )
}

fn truncate_utf8(value: &str, max_bytes: usize) -> &str {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
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
            version: JSON_VERSION,
            results: &rows
        })
        .expect("serializable output")
    )
}

pub fn list_text(rows: &[ResultRow], total: usize) -> io::Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(output, "{total} skills in the current inventory.")?;
    if total == 0 {
        return Ok(());
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
            version: JSON_VERSION,
            total,
            results: &rows,
        })
        .expect("serializable output")
    )
}

pub(crate) fn clean_row(mut row: ResultRow) -> ResultRow {
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
    fn row(description: String) -> ResultRow {
        ResultRow {
            id: "demo@abcdef".into(),
            name: "demo".into(),
            description,
            scope: "global".into(),
            path: "/skills/demo/SKILL.md".into(),
            canonical: "/skills/demo/SKILL.md".into(),
            base: "/skills/demo".into(),
            source: "/skills".into(),
            source_kind: "filesystem".into(),
            enabled: true,
            plugin_id: None,
            degraded: false,
            hash: "hash".into(),
        }
    }

    #[test]
    fn strips_terminal_controls() {
        assert_eq!(clean("safe\u{1b}[31m red\n"), "safe red");
    }

    #[test]
    fn oversized_description_keeps_complete_id_and_valid_utf8() {
        let line = bounded_line(&row("é".repeat(2_000)), MAX_RECORD_BYTES);
        assert!(line.starts_with("demo@abcdef [global] "));
        assert!(line.len() <= MAX_RECORD_BYTES);
        assert!(line.ends_with('\n'));
        assert!(line.contains("[truncated]"));
    }

    #[test]
    fn truncates_each_record_without_omitting_later_results() {
        let first = row("x".repeat(MAX_RECORD_BYTES));
        let mut second = row("second result".into());
        second.id = "second@abcdef".into();

        let lines = [first, second]
            .iter()
            .map(|row| bounded_line(row, MAX_RECORD_BYTES))
            .collect::<Vec<_>>();

        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("[truncated]"));
        assert!(lines[0].len() <= MAX_RECORD_BYTES);
        assert!(lines[1].contains("second@abcdef"));
        assert!(lines[1].contains("second result"));
    }

    #[test]
    fn uses_version_two_json_contract() {
        assert_eq!(JSON_VERSION, 2);
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
