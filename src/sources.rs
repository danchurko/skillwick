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
    let configured_targets: Vec<_> = extra
        .iter()
        .filter_map(|root| fs::canonicalize(root).ok())
        .collect();
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
            if !canonical_directory.starts_with(&authorized_root)
                && !configured_targets
                    .iter()
                    .any(|target| canonical_directory.starts_with(target))
            {
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
                if !canonical.starts_with(&authorized_root)
                    && !configured_targets
                        .iter()
                        .any(|target| canonical.starts_with(target))
                {
                    diagnostics.push(format!("symlink escape: {}", path.display()));
                    complete = false;
                    continue;
                }
                if !seen.insert(canonical.clone()) {
                    continue;
                }
                match metadata::parse(&canonical) {
                    Ok(metadata) => {
                        if let Some(diagnostic) = &metadata.policy_diagnostic {
                            diagnostics.push(format!("{}: {diagnostic}", path.display()));
                        }
                        skills.push(Skill {
                            base: path.parent().unwrap_or(&root).to_path_buf(),
                            path,
                            canonical,
                            scope: scope.clone(),
                            source: root.display().to_string(),
                            source_kind: "filesystem".into(),
                            enabled: true,
                            plugin_id: None,
                            metadata,
                        });
                    }
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

    #[cfg(unix)]
    #[test]
    fn follows_symlinks_into_an_explicitly_configured_root() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        let managed = temp.path().join("managed");
        fs::create_dir_all(home.join(".agents/skills")).unwrap();
        fs::create_dir_all(managed.join("reviewed")).unwrap();
        fs::write(
            managed.join("reviewed/SKILL.md"),
            "---\nname: reviewed\ndescription: managed skill\n---\n",
        )
        .unwrap();
        symlink(
            managed.join("reviewed"),
            home.join(".agents/skills/reviewed"),
        )
        .unwrap();
        let report = scan(&home, &[home.join(".agents/skills"), managed.clone()]);
        assert!(!report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("reviewed")));
        assert_eq!(
            report
                .skills
                .iter()
                .filter(|skill| skill.metadata.name == "reviewed")
                .count(),
            1
        );
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

    #[test]
    fn retains_policy_denials_as_raw_records_with_diagnostics() {
        let temp = tempfile::tempdir().unwrap();
        let skill = temp.path().join(".agents/skills/manual");
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: manual\ndescription: manual workflow\ndisable-model-invocation: maybe\n---\n",
        )
        .unwrap();

        let report = scan(temp.path(), &[]);
        let manual = report
            .skills
            .iter()
            .find(|skill| skill.metadata.name == "manual")
            .expect("manual skill missing");
        assert!(!manual.metadata.invocation_policy.model_discoverable());
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("invocation policy")));
    }
}
