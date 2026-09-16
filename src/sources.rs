use crate::{discovery, metadata};
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

/// Return the roots applicable to the current lookup context.
pub fn roots(report: &discovery::Report) -> Vec<(PathBuf, String)> {
    let mut roots = Vec::new();
    for source in &report.sources {
        let root = PathBuf::from(&source.root);
        if !roots
            .iter()
            .any(|(existing, scope)| existing == &root && scope == &source.scope)
        {
            roots.push((root, source.scope.clone()));
        }
    }
    roots
}

/// Return canonical root keys used by scoped index queries.
pub fn root_keys(report: &discovery::Report) -> Vec<String> {
    let mut keys = roots(report)
        .into_iter()
        .filter_map(|(root, _)| root.to_str().map(str::to_owned))
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    keys
}

/// Scan only source roots resolved by `discovery`.
///
/// `SourceSpec` values are already scoped to the requested workspace. This
/// scanner deliberately receives those resolved values instead of deriving
/// paths from `Config`, so a filesystem walk cannot broaden the source
/// boundary by accident.
pub fn scan(_cwd: &Path, report: &discovery::Report) -> ScanReport {
    let source_roots = roots(report);
    let configured_targets = source_roots
        .iter()
        .filter_map(|(root, _)| fs::canonicalize(root).ok())
        .collect::<Vec<_>>();
    let mut seen: HashMap<PathBuf, usize> = HashMap::new();
    let mut skills: Vec<Skill> = Vec::new();
    let mut diagnostics = Vec::new();
    let mut complete = true;
    let home = fs::canonicalize(crate::config::home()).ok();

    for source in &report.sources {
        let root = PathBuf::from(&source.root);
        if root.to_str().is_none() {
            diagnostics.push("source root is not UTF-8".to_owned());
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
        if authorized_root.to_str().is_none() {
            diagnostics.push(format!("source root is not UTF-8: {}", root.display()));
            complete = false;
            continue;
        }

        let mut stack = vec![authorized_root.clone()];
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
            if !within_any(
                &canonical_directory,
                &authorized_root,
                &configured_targets,
                allows_home_symlinks(&source.provider),
                home.as_deref(),
            ) {
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
                if file_type.is_dir() || (file_type.is_symlink() && path.is_dir()) {
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
                if !within_any(
                    &canonical,
                    &authorized_root,
                    &configured_targets,
                    allows_home_symlinks(&source.provider),
                    home.as_deref(),
                ) {
                    diagnostics.push(format!("symlink escape: {}", path.display()));
                    complete = false;
                    continue;
                }
                if path.to_str().is_none()
                    || canonical.to_str().is_none()
                    || path.parent().and_then(Path::to_str).is_none()
                {
                    diagnostics.push(format!("skill path is not UTF-8: {}", path.display()));
                    complete = false;
                    continue;
                }
                let (mut parsed, source_fingerprint) =
                    match metadata::parse_with_fingerprint(&canonical) {
                        Ok(parsed) => parsed,
                        Err(error) => {
                            diagnostics.push(format!("{}: {error}", path.display()));
                            complete = false;
                            continue;
                        }
                    };
                if let Some(diagnostic) = &parsed.policy_diagnostic {
                    diagnostics.push(format!("{}: {diagnostic}", path.display()));
                }
                namespace_plugin_name(&mut parsed, source.plugin_id.as_deref());
                let association = (authorized_root.clone(), source.scope.clone());
                if let Some(index) = seen.get(&canonical).copied() {
                    if !skills[index].roots.contains(&association) {
                        skills[index].roots.push(association);
                    }
                } else {
                    seen.insert(canonical.clone(), skills.len());
                    let base = path.parent().unwrap_or(&authorized_root).to_path_buf();
                    if base.to_str().is_none() {
                        diagnostics
                            .push(format!("skill base path is not UTF-8: {}", base.display()));
                        complete = false;
                        continue;
                    }
                    skills.push(Skill {
                        base,
                        path,
                        canonical,
                        scope: source.scope.clone(),
                        source: source.root.clone(),
                        source_kind: "filesystem".into(),
                        enabled: true,
                        plugin_id: source.plugin_id.clone(),
                        source_fingerprint,
                        roots: vec![association],
                        metadata: parsed,
                    });
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
            .then(left.plugin_id.cmp(&right.plugin_id))
    });
    ScanReport {
        skills,
        diagnostics,
        complete,
    }
}

fn within_any(
    candidate: &Path,
    authorized: &Path,
    configured: &[PathBuf],
    allow_home_symlink: bool,
    home: Option<&Path>,
) -> bool {
    candidate.starts_with(authorized)
        || configured
            .iter()
            .any(|target| candidate.starts_with(target))
        || (allow_home_symlink && home.is_some_and(|home| candidate.starts_with(home)))
}

fn allows_home_symlinks(provider: &str) -> bool {
    matches!(provider, "agents" | "codex" | "claude")
}

/// Namespace plugin skill names using the provider's stable plugin name.
///
/// Claude and Codex expose plugin identity as `name@marketplace`, while their
/// instruction frontmatter commonly contains only the local skill name. The
/// host-facing exact-name contract uses `name:skill`; avoid adding a second
/// prefix when a package has already supplied that qualified name.
fn namespace_plugin_name(parsed: &mut metadata::Metadata, plugin_id: Option<&str>) {
    let Some(plugin_id) = plugin_id else {
        return;
    };
    let namespace = plugin_id
        .split_once('@')
        .map(|(name, _)| name)
        .unwrap_or(plugin_id);
    if namespace.is_empty()
        || parsed.name == namespace
        || parsed.name.starts_with(&format!("{namespace}:"))
    {
        if parsed.name == namespace {
            parsed.name = format!("{namespace}:{namespace}");
        }
        return;
    }
    parsed.name = format!("{namespace}:{}", parsed.name);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::SourceSpec;

    fn report(root: &Path, plugin_id: Option<&str>) -> discovery::Report {
        discovery::Report {
            sources: vec![SourceSpec {
                provider: if plugin_id.is_some() {
                    "claude-plugin".into()
                } else {
                    "custom".into()
                },
                scope: "global".into(),
                root: root.to_str().unwrap().into(),
                plugin_id: plugin_id.map(str::to_owned),
                version: plugin_id.map(|_| "1".into()),
                provenance: "fixture".into(),
            }],
            configured_sources: Vec::new(),
            configured_roots: Vec::new(),
            diagnostics: Vec::new(),
            complete: true,
        }
    }

    #[test]
    fn scans_only_resolved_roots_without_ancestor_leakage() {
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
        let report = report(&project.join(".agents/skills"), None);
        let scanned = scan(&project, &report);
        assert!(scanned
            .skills
            .iter()
            .any(|skill| skill.metadata.name == "local"));
        assert!(!scanned
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
        let empty = discovery::Report::default();
        let scanned = scan(&cwd, &empty);
        assert!(scanned.complete);
        assert!(scanned.skills.is_empty());
    }

    #[test]
    fn project_roots_are_supplied_by_discovery() {
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
        let source = SourceSpec {
            provider: "custom".into(),
            scope: "project".into(),
            root: project_root.to_str().unwrap().into(),
            plugin_id: None,
            version: None,
            provenance: "fixture".into(),
        };
        let configured = discovery::Report {
            sources: vec![source],
            configured_sources: Vec::new(),
            configured_roots: Vec::new(),
            diagnostics: Vec::new(),
            complete: true,
        };
        assert_eq!(scan(&child, &configured).skills.len(), 1);
        // Applicability is established by discovery before this scanner runs.
        let outside = discovery::Report::default();
        assert!(scan(&sibling, &outside).skills.is_empty());
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
        let scanned = scan(&root, &report(&root, None));
        assert!(!scanned.complete);
        assert!(!scanned
            .skills
            .iter()
            .any(|skill| skill.metadata.name == "escaped"));
        assert!(scanned
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
        let first = SourceSpec {
            provider: "custom".into(),
            scope: "global".into(),
            root: home.join(".agents/skills").to_str().unwrap().into(),
            plugin_id: None,
            version: None,
            provenance: "fixture".into(),
        };
        let second = SourceSpec {
            provider: "custom".into(),
            scope: "global".into(),
            root: managed.to_str().unwrap().into(),
            plugin_id: None,
            version: None,
            provenance: "fixture".into(),
        };
        let sources = discovery::Report {
            sources: vec![first, second],
            configured_sources: Vec::new(),
            configured_roots: Vec::new(),
            diagnostics: Vec::new(),
            complete: true,
        };
        let scanned = scan(&home, &sources);
        assert!(!scanned
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("reviewed")));
        assert_eq!(
            scanned
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
        let source = SourceSpec {
            provider: "custom".into(),
            scope: "global".into(),
            root: missing.to_str().unwrap().into(),
            plugin_id: None,
            version: None,
            provenance: "fixture".into(),
        };
        let report = discovery::Report {
            sources: vec![source],
            configured_sources: Vec::new(),
            configured_roots: Vec::new(),
            diagnostics: Vec::new(),
            complete: true,
        };
        let scanned = scan(temp.path(), &report);
        assert!(!scanned.complete);
        assert!(scanned
            .diagnostics
            .iter()
            .any(|item| item.contains("missing")));
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

        let scanned = scan(
            temp.path(),
            &report(&temp.path().join(".agents/skills"), None),
        );
        let manual = scanned
            .skills
            .iter()
            .find(|skill| skill.metadata.name == "manual")
            .expect("manual skill missing");
        assert!(!manual.metadata.invocation_policy.model_discoverable());
        assert!(scanned
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("invocation policy")));
    }

    #[test]
    fn plugin_skill_names_use_provider_namespace() {
        let temp = tempfile::tempdir().unwrap();
        let skill = temp.path().join("skills/ponytail");
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: ponytail\ndescription: lazy mode\n---\n",
        )
        .unwrap();
        let scanned = scan(
            temp.path(),
            &report(&temp.path().join("skills"), Some("ponytail@ponytail")),
        );
        assert_eq!(scanned.skills[0].metadata.name, "ponytail:ponytail");
    }

    #[test]
    fn already_qualified_plugin_names_are_stable() {
        let mut metadata = metadata::Metadata {
            name: "ponytail:ponytail".into(),
            description: "".into(),
            keywords: "".into(),
            degraded: false,
            hash: "".into(),
            invocation_policy: metadata::InvocationPolicy::Discoverable,
            policy_diagnostic: None,
        };
        namespace_plugin_name(&mut metadata, Some("ponytail@ponytail"));
        assert_eq!(metadata.name, "ponytail:ponytail");
    }
}
