use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

pub const MAX_FILE: usize = 1024 * 1024;
const MAX_FRONTMATTER: usize = 64 * 1024;
const MAX_DESCRIPTION: usize = 8 * 1024;

#[derive(Debug, Deserialize)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
    keywords: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct Metadata {
    pub name: String,
    pub description: String,
    pub keywords: String,
    pub degraded: bool,
    pub hash: String,
}

pub fn parse(path: &Path) -> Result<Metadata, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FILE {
        return Err("instruction file exceeds 1 MiB".into());
    }
    let raw =
        std::str::from_utf8(&bytes).map_err(|_| "instruction file is not UTF-8".to_string())?;
    let text = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let parsed = match frontmatter(text)? {
        Some(frontmatter) => {
            if frontmatter.len() > MAX_FRONTMATTER {
                return Err("frontmatter exceeds 64 KiB".into());
            }
            reject_duplicate_keys(frontmatter)?;
            serde_saphyr::from_str::<Frontmatter>(frontmatter)
                .map_err(|e| format!("invalid YAML frontmatter: {e}"))?
        }
        None => Frontmatter {
            name: None,
            description: None,
            keywords: None,
        },
    };
    let name = parsed
        .name
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            path.parent()
                .and_then(Path::file_name)
                .and_then(|value| value.to_str())
                .unwrap_or("skill")
                .to_owned()
        });
    let mut degraded = parsed
        .description
        .as_deref()
        .is_none_or(|value| value.trim().is_empty());
    let description = parsed
        .description
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| name.clone());
    let description = if description.len() > MAX_DESCRIPTION {
        degraded = true;
        let mut end = MAX_DESCRIPTION.min(description.len());
        while !description.is_char_boundary(end) {
            end -= 1;
        }
        description[..end].to_owned()
    } else {
        description
    };
    Ok(Metadata {
        name,
        description,
        keywords: parsed.keywords.unwrap_or_default().join(" "),
        degraded,
        hash: format!("{:x}", Sha256::digest(&bytes)),
    })
}

fn frontmatter(text: &str) -> Result<Option<&str>, String> {
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return Ok(None);
    };
    for marker in ["\n---\n", "\r\n---\r\n", "\n---\r\n"] {
        if let Some(end) = rest.find(marker) {
            return Ok(Some(&rest[..end]));
        }
    }
    Err("unterminated YAML frontmatter".into())
}

fn reject_duplicate_keys(frontmatter: &str) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for line in frontmatter.lines() {
        if line.starts_with(char::is_whitespace) || line.trim_start().starts_with('#') {
            continue;
        }
        if let Some((key, _)) = line.split_once(':') {
            let key = key.trim();
            if !key.is_empty() && !seen.insert(key) {
                return Err(format!("duplicate frontmatter key: {key}"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_bom_crlf_folded_unicode_and_rejects_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("SKILL.md");
        fs::write(&path, "\u{feff}---\r\nname: C++\r\ndescription: >-\r\n  Build café\r\n  tools\r\nkeywords: [C#, .NET, Node.js]\r\n---\r\nbody").unwrap();
        let metadata = parse(&path).unwrap();
        assert_eq!(metadata.name, "C++");
        assert!(metadata.description.contains("café tools"));
        assert!(metadata.keywords.contains("Node.js"));
        fs::write(&path, "---\nname: one\nname: two\n---\nbody").unwrap();
        assert!(parse(&path).unwrap_err().contains("duplicate"));
    }
}
