use crate::metadata;
use std::{
    collections::{HashMap, HashSet},
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
    pub source_fingerprint: String,
    pub roots: Vec<(PathBuf, String)>,
    pub metadata: metadata::Metadata,
}

pub struct ScanReport {
    pub skills: Vec<Skill>,
    pub diagnostics: Vec<String>,
    pub complete: bool,
}

pub fn roots(
    cwd: &Path,
    shared: &[PathBuf],
    projects: &[crate::config::Project],
) -> Vec<(PathBuf, String)> {
    let normalized_cwd = fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let mut roots = shared
        .iter()
        .cloned()
        .map(|path| (path, "global".to_owned()))
        .collect::<Vec<_>>();
    for project in projects {
        let Ok(path) = fs::canonicalize(&project.path) else {
            continue;
        };
        if normalized_cwd.starts_with(path) {
            roots.extend(
                project
                    .roots
                    .iter()
                    .cloned()
                    .map(|root| (root, "project".to_owned())),
            );
        }
    }
    let mut seen = HashSet::new();
    roots
        .into_iter()
        .filter(|(path, _)| {
            let identity = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            seen.insert(identity)
        })
        .collect()
}

pub fn root_keys(
    cwd: &Path,
    shared: &[PathBuf],
    projects: &[crate::config::Project],
) -> Vec<String> {
    roots(cwd, shared, projects)
        .into_iter()
        .map(|(root, _)| {
            fs::canonicalize(&root)
                .unwrap_or(root)
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

pub fn configured_roots(
    shared: &[PathBuf],
    projects: &[crate::config::Project],
) -> Vec<(String, String)> {
    let mut configured = shared
        .iter()
        .map(|root| ("shared".to_owned(), canonical_key(root)))
        .collect::<Vec<_>>();
    for project in projects {
        let project_key = canonical_key(&project.path);
        configured.extend(
            project
                .roots
                .iter()
                .map(|root| (format!("project:{project_key}"), canonical_key(root))),
        );
    }
    configured.sort();
    configured.dedup();
    configured
}

fn canonical_key(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

pub fn scan(cwd: &Path, shared: &[PathBuf], projects: &[crate::config::Project]) -> ScanReport {
    let configured_targets: Vec<_> = roots(cwd, shared, projects)
        .iter()
        .filter_map(|(root, _)| fs::canonicalize(root).ok())
        .collect();
    let mut seen: HashMap<PathBuf, usize> = HashMap::new();
    let mut skills: Vec<Skill> = Vec::new();
    let mut diagnostics = Vec::new();
    let mut complete = true;
    for (root, scope) in roots(cwd, shared, projects) {
        if !root.exists() {
            diagnostics.push(format!(
                "configured root does not exist: {}",
                root.display()
            ));
            complete = false;
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
                match metadata::parse(&canonical) {
                    Ok(metadata) => {
                        if let Some(diagnostic) = &metadata.policy_diagnostic {
                            diagnostics.push(format!("{}: {diagnostic}", path.display()));
                        }
                        let source_fingerprint =
                            metadata::source_fingerprint(&canonical).map_err(|error| {
                                diagnostics.push(format!("{}: {error}", path.display()));
                                complete = false;
                            });
                        let Ok(source_fingerprint) = source_fingerprint else {
                            continue;
                        };
                        let association = (authorized_root.clone(), scope.clone());
                        if let Some(index) = seen.get(&canonical).copied() {
                            skills[index].roots.push(association);
                        } else {
                            seen.insert(canonical.clone(), skills.len());
                            skills.push(Skill {
                                base: path.parent().unwrap_or(&root).to_path_buf(),
                                path,
                                canonical,
                                scope: scope.clone(),
                                source: root.display().to_string(),
                                source_kind: "filesystem".into(),
                                enabled: true,
                                plugin_id: None,
                                source_fingerprint,
                                roots: vec![association],
                                metadata,
                            });
                        }
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
        let report = scan(
            &project,
            &[],
            &[crate::config::Project {
                path: parent.join("one"),
                roots: vec![project.join(".agents/skills")],
            }],
        );
        assert!(report
            .skills
            .iter()
            .any(|skill| skill.metadata.name == "local"));
        assert!(!report
            .skills
            .iter()
            .any(|skill| skill.metadata.name == "leak"));
    }

    #[test]
    fn no_implicit_home_or_ancestor_roots_are_scanned() {
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().join("project/child");
        let ancestor = temp.path().join("project/.agents/skills/ancestor");
        let home = temp.path().join("home/.agents/skills/home");
        fs::create_dir_all(&cwd).unwrap();
        fs::create_dir_all(&ancestor).unwrap();
        fs::create_dir_all(&home).unwrap();
        fs::write(
            ancestor.join("SKILL.md"),
            "---\nname: ancestor\ndescription: no\n---\n",
        )
        .unwrap();
        fs::write(
            home.join("SKILL.md"),
            "---\nname: home\ndescription: no\n---\n",
        )
        .unwrap();
        let report = scan(&cwd, &[], &[]);
        assert!(report.complete);
        assert!(report.skills.is_empty());
    }

    #[test]
    fn project_roots_apply_to_descendants_only() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let child = project.join("nested/child");
        let sibling = temp.path().join("sibling");
        let project_root = temp.path().join("project-skills");
        fs::create_dir_all(&child).unwrap();
        fs::create_dir_all(&sibling).unwrap();
        fs::create_dir_all(project_root.join("skill")).unwrap();
        fs::write(
            project_root.join("skill/SKILL.md"),
            "---\nname: project\ndescription: project\n---\n",
        )
        .unwrap();
        let configured = [crate::config::Project {
            path: project,
            roots: vec![project_root],
        }];
        assert_eq!(scan(&child, &[], &configured).skills.len(), 1);
        assert!(scan(&sibling, &[], &configured).skills.is_empty());
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
        let report = scan(temp.path(), &[root], &[]);
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
        let report = scan(&home, &[home.join(".agents/skills"), managed.clone()], &[]);
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
        let report = scan(temp.path(), std::slice::from_ref(&missing), &[]);
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

        let report = scan(temp.path(), &[temp.path().join(".agents/skills")], &[]);
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
