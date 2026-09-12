use crate::metadata;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct Skill {
    pub path: PathBuf,
    pub canonical: PathBuf,
    pub base: PathBuf,
    pub scope: String,
    pub source: String,
    pub source_kind: String,
    pub enabled: bool,
    pub plugin_id: Option<String>,
    pub metadata: metadata::Metadata,
}

pub struct ScanReport {
    pub skills: Vec<Skill>,
    pub diagnostics: Vec<String>,
    pub complete: bool,
}

pub fn roots(cwd: &Path, extra: &[PathBuf]) -> Vec<(PathBuf, String)> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        roots.push((PathBuf::from(home).join(".agents/skills"), "global".into()));
    }
    for directory in cwd.ancestors() {
        roots.push((
            directory.join(".agents/skills"),
            if directory == cwd {
                "project".into()
            } else {
                "ancestor".into()
            },
        ));
    }
    roots.extend(extra.iter().cloned().map(|path| (path, "custom".into())));
    let mut seen = HashSet::new();
    roots
        .into_iter()
        .filter(|(path, _)| seen.insert(path.clone()))
        .collect()
}

pub fn scan(cwd: &Path, extra: &[PathBuf]) -> ScanReport {
    let configured: HashSet<_> = extra.iter().collect();
    let mut seen = HashSet::new();
    let mut skills = Vec::new();
    let mut diagnostics = Vec::new();
    let mut complete = true;
    for (root, scope) in roots(cwd, extra) {
        if !root.exists() {
            if configured.contains(&root) {
                diagnostics.push(format!(
                    "configured root does not exist: {}",
                    root.display()
                ));
                complete = false;
            }
            continue;
        }
        let authorized_root = match fs::canonicalize(&root) {
            Ok(path) => path,
            Err(error) => {
                diagnostics.push(format!("{}: {error}", root.display()));
                complete = false;
                continue;
            }
        };
        if !authorized_root.is_dir() {
            diagnostics.push(format!("not a directory: {}", root.display()));
            complete = false;
            continue;
        }
        let mut stack = vec![root.clone()];
        let mut visited = HashSet::new();
        while let Some(directory) = stack.pop() {
            let canonical_directory = match fs::canonicalize(&directory) {
                Ok(path) => path,
                Err(error) => {
                    diagnostics.push(format!("{}: {error}", directory.display()));
                    complete = false;
                    continue;
                }
            };
            if !canonical_directory.starts_with(&authorized_root) {
                diagnostics.push(format!("symlink escape: {}", directory.display()));
                complete = false;
                continue;
            }
            if !visited.insert(canonical_directory) {
                continue;
            }
            let entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) => {
                    diagnostics.push(format!("{}: {error}", directory.display()));
                    complete = false;
                    continue;
                }
            };
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        diagnostics.push(error.to_string());
                        complete = false;
                        continue;
                    }
                };
                let path = entry.path();
                let file_type = match entry.file_type() {
                    Ok(kind) => kind,
                    Err(error) => {
                        diagnostics.push(format!("{}: {error}", path.display()));
                        complete = false;
                        continue;
                    }
                };
                if file_type.is_dir() || file_type.is_symlink() && path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.file_name().and_then(|name| name.to_str()) != Some("SKILL.md") {
                    continue;
                }
                let canonical = match fs::canonicalize(&path) {
                    Ok(path) => path,
                    Err(error) => {
                        diagnostics.push(format!("{}: {error}", path.display()));
                        complete = false;
                        continue;
                    }
                };
                if !canonical.starts_with(&authorized_root) {
                    diagnostics.push(format!("symlink escape: {}", path.display()));
                    complete = false;
                    continue;
                }
                if !seen.insert(canonical.clone()) {
                    continue;
                }
                match metadata::parse(&canonical) {
                    Ok(metadata) => skills.push(Skill {
                        base: path.parent().unwrap_or(&root).to_path_buf(),
                        path,
                        canonical,
                        scope: scope.clone(),
                        source: root.display().to_string(),
                        source_kind: "filesystem".into(),
                        enabled: true,
                        plugin_id: None,
                        metadata,
                    }),
                    Err(error) => {
                        diagnostics.push(format!("{}: {error}", path.display()));
                        complete = false;
                    }
                }
            }
        }
    }
    skills.sort_by(|left, right| {
        left.metadata
            .name
            .to_lowercase()
            .cmp(&right.metadata.name.to_lowercase())
            .then(left.canonical.cmp(&right.canonical))
    });
    ScanReport {
        skills,
        diagnostics,
        complete,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scopes_ancestors_without_leaking_sibling_projects() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path();
        let project = parent.join("one/work");
        let sibling = parent.join("two");
        fs::create_dir_all(project.join(".agents/skills/local")).unwrap();
        fs::create_dir_all(sibling.join(".agents/skills/leak")).unwrap();
        fs::write(
            project.join(".agents/skills/local/SKILL.md"),
            "---\nname: local\ndescription: local skill\n---\n",
        )
        .unwrap();
        fs::write(
            sibling.join(".agents/skills/leak/SKILL.md"),
            "---\nname: leak\ndescription: must not appear\n---\n",
        )
        .unwrap();
        let report = scan(&project, &[]);
        assert!(report
            .skills
            .iter()
            .any(|skill| skill.metadata.name == "local"));
        assert!(!report
            .skills
            .iter()
            .any(|skill| skill.metadata.name == "leak"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape_and_cycles() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let outside = temp.path().join("outside/skill");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(
            outside.join("SKILL.md"),
            "---\nname: escaped\ndescription: no\n---\n",
        )
        .unwrap();
        symlink(&outside, root.join("escape")).unwrap();
        symlink(&root, root.join("cycle")).unwrap();
        let report = scan(temp.path(), &[root]);
        assert!(!report.complete);
        assert!(!report
            .skills
            .iter()
            .any(|skill| skill.metadata.name == "escaped"));
        assert!(report
            .diagnostics
            .iter()
            .any(|item| item.contains("symlink escape")));
    }

    #[test]
    fn missing_configured_root_makes_scan_incomplete() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing");
        let report = scan(temp.path(), std::slice::from_ref(&missing));
        assert!(!report.complete);
        assert!(report
            .diagnostics
            .iter()
            .any(|item| item == &format!("configured root does not exist: {}", missing.display())));
    }
}
