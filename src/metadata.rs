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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvocationPolicy {
    Discoverable,
    Denied,
}

impl InvocationPolicy {
    pub fn model_discoverable(self) -> bool {
        matches!(self, Self::Discoverable)
    }
}

#[derive(Debug, Clone)]
pub struct Metadata {
    pub name: String,
    pub description: String,
    pub keywords: String,
    pub degraded: bool,
    pub hash: String,
    pub invocation_policy: InvocationPolicy,
    pub policy_diagnostic: Option<String>,
}

pub fn parse(path: &Path) -> Result<Metadata, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() > MAX_FILE {
        return Err("instruction file exceeds 1 MiB".into());
    }
    let raw =
        std::str::from_utf8(&bytes).map_err(|_| "instruction file is not UTF-8".to_string())?;
    let text = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let mut policy_diagnostic = None;
    let frontmatter_text = frontmatter(text)?;
    let (parsed, policy_text) = match frontmatter_text {
        Some(frontmatter) => {
            if frontmatter.len() > MAX_FRONTMATTER {
                return Err("frontmatter exceeds 64 KiB".into());
            }
            let policy_text = match reject_duplicate_keys(frontmatter) {
                Ok(()) => frontmatter.to_owned(),
                Err(error) if is_invocation_key_error(&error) => {
                    policy_diagnostic = Some(format!("invocation policy: {error}"));
                    without_duplicate_invocation_keys(frontmatter)
                }
                Err(error) => return Err(error),
            };
            let parsed = match serde_saphyr::from_str::<Frontmatter>(&policy_text) {
                Ok(parsed) => parsed,
                Err(error) if contains_invocation_key(frontmatter) => {
                    policy_diagnostic = Some(format!(
                        "invocation policy: invalid YAML frontmatter: {error}"
                    ));
                    Frontmatter {
                        name: None,
                        description: None,
                        keywords: None,
                    }
                }
                Err(error) => return Err(format!("invalid YAML frontmatter: {error}")),
            };
            (parsed, Some(policy_text))
        }
        None => (
            Frontmatter {
                name: None,
                description: None,
                keywords: None,
            },
            None,
        ),
    };
    let mut invocation_policy = InvocationPolicy::Discoverable;
    let policy_document = match frontmatter_document(policy_text.as_deref()) {
        Ok(document) => document,
        Err(error) if policy_diagnostic.is_some() => {
            policy_diagnostic.get_or_insert(error);
            None
        }
        Err(error) => return Err(error),
    };
    if let Some(value) = recognized_bool(
        policy_document.as_ref(),
        "disable-model-invocation",
        &mut policy_diagnostic,
    ) {
        if value {
            invocation_policy = InvocationPolicy::Denied;
        }
    }
    let (sidecar_policy, sidecar_diagnostic) = adjacent_policy(path);
    if let Some(diagnostic) = sidecar_diagnostic {
        policy_diagnostic.get_or_insert(diagnostic);
    }
    if matches!(sidecar_policy, Some(false)) {
        invocation_policy = InvocationPolicy::Denied;
    }
    if policy_diagnostic.is_some() {
        invocation_policy = InvocationPolicy::Denied;
    }
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
        invocation_policy,
        policy_diagnostic,
    })
}

fn adjacent_policy(path: &Path) -> (Option<bool>, Option<String>) {
    let policy_path = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("agents/openai.yaml");
    let bytes = match fs::read(&policy_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return (None, None),
        Err(error) => {
            return (
                None,
                Some(format!(
                    "invocation policy: unable to read {}: {error}",
                    policy_path.display()
                )),
            )
        }
    };
    if bytes.len() > MAX_FRONTMATTER {
        return (
            None,
            Some(format!(
                "invocation policy: {} exceeds 64 KiB",
                policy_path.display()
            )),
        );
    }
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text,
        Err(_) => {
            return (
                None,
                Some(format!(
                    "invocation policy: {} is not UTF-8",
                    policy_path.display()
                )),
            )
        }
    };
    if let Some(key) = duplicate_invocation_key(text) {
        return (
            None,
            Some(format!(
                "invocation policy: duplicate recognized key: {key}"
            )),
        );
    }
    let parsed: serde_json::Value = match serde_saphyr::from_str(text) {
        Ok(parsed) => parsed,
        Err(error) => {
            return (
                None,
                Some(format!(
                    "invocation policy: invalid agents/openai.yaml: {error}"
                )),
            )
        }
    };
    let Some(document) = parsed.as_object() else {
        return (
            None,
            Some("invocation policy: agents/openai.yaml must be a mapping".into()),
        );
    };
    let Some(policy) = document.get("policy") else {
        return (None, None);
    };
    let Some(policy) = policy.as_object() else {
        return (
            None,
            Some("invocation policy: policy must be a mapping".into()),
        );
    };
    let Some(value) = policy.get("allow_implicit_invocation") else {
        return (None, None);
    };
    match value {
        serde_json::Value::Bool(value) => (Some(*value), None),
        _ => (
            None,
            Some("invocation policy: policy.allow_implicit_invocation must be boolean".into()),
        ),
    }
}

fn frontmatter_document(text: Option<&str>) -> Result<Option<serde_json::Value>, String> {
    text.map(|text| {
        serde_saphyr::from_str(text).map_err(|error| format!("invalid YAML frontmatter: {error}"))
    })
    .transpose()
}

fn recognized_bool(
    document: Option<&serde_json::Value>,
    key: &str,
    diagnostic: &mut Option<String>,
) -> Option<bool> {
    let document = document.and_then(serde_json::Value::as_object)?;
    let value = document.get(key)?;
    match value {
        serde_json::Value::Bool(value) => Some(*value),
        _ => {
            diagnostic.get_or_insert_with(|| format!("invocation policy: {key} must be boolean"));
            None
        }
    }
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

fn is_invocation_key_error(error: &str) -> bool {
    ["disable-model-invocation", "allow_implicit_invocation"]
        .iter()
        .any(|key| error.ends_with(key))
}

fn duplicate_invocation_key(text: &str) -> Option<&str> {
    let recognized = ["disable-model-invocation", "allow_implicit_invocation"];
    let mut seen = std::collections::HashSet::new();
    for line in text.lines() {
        let line = line.trim_start();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, _)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if recognized.contains(&key) && !seen.insert(key) {
            return Some(key);
        }
    }
    None
}

fn contains_invocation_key(text: &str) -> bool {
    text.lines().any(|line| {
        let key = line.trim_start().split_once(':').map(|(key, _)| key.trim());
        matches!(key, Some("disable-model-invocation"))
    })
}

fn without_duplicate_invocation_keys(text: &str) -> String {
    let recognized = ["disable-model-invocation", "allow_implicit_invocation"];
    let mut seen = std::collections::HashSet::new();
    text.lines()
        .filter(|line| {
            let key = line.trim_start().split_once(':').map(|(key, _)| key.trim());
            match key {
                Some(key) if recognized.contains(&key) => seen.insert(key),
                _ => true,
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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

    #[test]
    fn normalizes_invocation_policy_without_confusing_user_visibility() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("SKILL.md");
        fs::write(
            &path,
            "---\nname: manual\ndescription: manual\ndisable-model-invocation: true\n---\n",
        )
        .unwrap();
        assert_eq!(
            parse(&path).unwrap().invocation_policy,
            InvocationPolicy::Denied
        );

        fs::write(
            &path,
            "---\nname: menu-hidden\ndescription: discoverable\nuser-invocable: false\n---\n",
        )
        .unwrap();
        assert_eq!(
            parse(&path).unwrap().invocation_policy,
            InvocationPolicy::Discoverable
        );

        fs::write(
            &path,
            "---\nname: malformed-user-visibility\ndescription: discoverable\nuser-invocable: maybe\n---\n",
        )
        .unwrap();
        let metadata = parse(&path).unwrap();
        assert_eq!(metadata.invocation_policy, InvocationPolicy::Discoverable);
        assert!(metadata.policy_diagnostic.is_none());
    }

    #[test]
    fn normalizes_adjacent_codex_policy_and_fails_closed_on_bad_values() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("skill");
        fs::create_dir_all(package.join("agents")).unwrap();
        let path = package.join("SKILL.md");
        fs::write(&path, "---\nname: policy\ndescription: policy\n---\n").unwrap();
        fs::write(
            package.join("agents/openai.yaml"),
            "policy:\n  allow_implicit_invocation: false\n",
        )
        .unwrap();
        assert_eq!(
            parse(&path).unwrap().invocation_policy,
            InvocationPolicy::Denied
        );

        fs::write(
            package.join("agents/openai.yaml"),
            "policy:\n  allow_implicit_invocation: maybe\n",
        )
        .unwrap();
        let metadata = parse(&path).unwrap();
        assert_eq!(metadata.invocation_policy, InvocationPolicy::Denied);
        assert!(metadata
            .policy_diagnostic
            .as_deref()
            .is_some_and(|diagnostic| diagnostic.contains("allow_implicit_invocation")));

        fs::write(
            &path,
            "---\nname: null-policy\ndescription: policy\ndisable-model-invocation:\n---\n",
        )
        .unwrap();
        let metadata = parse(&path).unwrap();
        assert_eq!(metadata.invocation_policy, InvocationPolicy::Denied);
        assert!(metadata
            .policy_diagnostic
            .as_deref()
            .is_some_and(|diagnostic| diagnostic.contains("disable-model-invocation")));

        fs::write(
            &path,
            "---\nname: broken-policy\ndisable-model-invocation: [true\n---\n",
        )
        .unwrap();
        let metadata = parse(&path).unwrap();
        assert_eq!(metadata.invocation_policy, InvocationPolicy::Denied);
        assert!(metadata
            .policy_diagnostic
            .as_deref()
            .is_some_and(|diagnostic| diagnostic.contains("invalid YAML frontmatter")));

        fs::write(
            &path,
            "---\nname: conflicting\ndescription: policy\ndisable-model-invocation: true\ndisable-model-invocation: false\n---\n",
        )
        .unwrap();
        let metadata = parse(&path).unwrap();
        assert_eq!(metadata.invocation_policy, InvocationPolicy::Denied);
        assert!(metadata
            .policy_diagnostic
            .as_deref()
            .is_some_and(|diagnostic| diagnostic.contains("duplicate")));

        fs::write(
            &path,
            "---\nname: sidecar-conflicting\ndescription: policy\n---\n",
        )
        .unwrap();
        fs::write(
            package.join("agents/openai.yaml"),
            "policy:\n  allow_implicit_invocation: true\n  allow_implicit_invocation: false\n",
        )
        .unwrap();
        let metadata = parse(&path).unwrap();
        assert_eq!(metadata.invocation_policy, InvocationPolicy::Denied);
        assert!(metadata
            .policy_diagnostic
            .as_deref()
            .is_some_and(|diagnostic| diagnostic.contains("duplicate")));
    }
}
