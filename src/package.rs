use serde::Serialize;
use std::{ffi::OsStr, fs, path::Path};

pub const MAX_ENTRIES: usize = 256;
pub const MAX_DEPTH: usize = 32;
pub const MAX_PATH_BYTES: usize = 4096;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub file_type: String,
    pub classification: String,
    pub extension: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct Counts {
    pub total_entries: usize,
    pub regular_files: usize,
    pub additional_files: usize,
    pub markdown_files: usize,
    pub non_markdown_files: usize,
    pub directories: usize,
    pub symlinks: usize,
    pub other: usize,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub entries: Vec<Entry>,
    pub truncated: bool,
    pub counts: Counts,
}

pub fn inspect(root: &Path) -> Result<Report, String> {
    let mut stack = vec![(root.to_path_buf(), 0)];
    let mut entries = Vec::new();
    let mut counts = Counts::default();
    let mut truncated = false;

    while let Some((directory, depth)) = stack.pop() {
        if entries.len() >= MAX_ENTRIES {
            truncated = true;
            break;
        }
        let directory_entries = fs::read_dir(&directory)
            .map_err(|error| format!("{}: {error}", directory.display()))?;
        for directory_entry in directory_entries {
            if entries.len() >= MAX_ENTRIES {
                truncated = true;
                break;
            }
            let directory_entry =
                directory_entry.map_err(|error| format!("{}: {error}", directory.display()))?;
            let path = directory_entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| format!("package entry escaped {}", root.display()))?;
            let relative_text = relative.to_string_lossy().into_owned();
            if relative_text.len() > MAX_PATH_BYTES {
                truncated = true;
                continue;
            }
            let file_type = directory_entry
                .file_type()
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let file_type_name = if file_type.is_dir() {
                "directory"
            } else if file_type.is_symlink() {
                "symlink"
            } else if file_type.is_file() {
                "file"
            } else {
                "other"
            };
            let classification = if file_type.is_dir() {
                "directory"
            } else if path
                .extension()
                .and_then(OsStr::to_str)
                .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
            {
                "markdown"
            } else {
                "non-markdown"
            };
            let extension = path.extension().and_then(OsStr::to_str).map(str::to_owned);
            counts.total_entries += 1;
            if file_type.is_dir() {
                counts.directories += 1;
            } else if file_type.is_symlink() {
                counts.symlinks += 1;
            } else if file_type.is_file() {
                counts.regular_files += 1;
                if relative == Path::new("SKILL.md") {
                    if classification == "markdown" {
                        counts.markdown_files += 1;
                    } else {
                        counts.non_markdown_files += 1;
                    }
                } else {
                    counts.additional_files += 1;
                    if classification == "markdown" {
                        counts.markdown_files += 1;
                    } else {
                        counts.non_markdown_files += 1;
                    }
                }
            } else {
                counts.other += 1;
            }
            entries.push(Entry {
                path: relative_text,
                file_type: file_type_name.into(),
                classification: classification.into(),
                extension,
            });

            // `file_type` does not follow symlinks. Only real directories are
            // traversed, so a package cannot make inspection walk elsewhere.
            if file_type.is_dir() {
                if depth >= MAX_DEPTH {
                    truncated = true;
                } else {
                    stack.push((path, depth + 1));
                }
            }
        }
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(Report {
        entries,
        truncated,
        counts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn lists_relative_entries_and_classifies_extensions() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("skill");
        fs::create_dir_all(root.join("references")).unwrap();
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::write(root.join("SKILL.md"), "instructions").unwrap();
        fs::write(root.join("references/guide.md"), "reference").unwrap();
        fs::write(root.join("scripts/check.py"), "print('check')").unwrap();

        let report = inspect(&root).unwrap();
        assert!(!report.truncated);
        assert_eq!(
            report.counts,
            Counts {
                total_entries: 5,
                regular_files: 3,
                additional_files: 2,
                markdown_files: 2,
                non_markdown_files: 1,
                directories: 2,
                symlinks: 0,
                other: 0,
            }
        );
        assert!(report.entries.iter().any(|entry| {
            entry.path == "SKILL.md"
                && entry.file_type == "file"
                && entry.classification == "markdown"
        }));
        assert!(report.entries.iter().any(|entry| {
            entry.path == "scripts/check.py"
                && entry.classification == "non-markdown"
                && entry.extension.as_deref() == Some("py")
        }));
        assert!(report
            .entries
            .iter()
            .all(|entry| !Path::new(&entry.path).is_absolute()));
    }

    #[cfg(unix)]
    #[test]
    fn reports_symlink_entries_without_following_escape_directories() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("skill");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "must not be read").unwrap();
        symlink(&outside, root.join("references")).unwrap();

        let report = inspect(&root).unwrap();
        assert!(report
            .entries
            .iter()
            .any(|entry| entry.path == "references" && entry.file_type == "symlink"));
        assert!(!report
            .entries
            .iter()
            .any(|entry| entry.path.contains("secret.txt")));
    }

    #[test]
    fn marks_entry_limit_truncation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("skill");
        fs::create_dir_all(&root).unwrap();
        for index in 0..=MAX_ENTRIES {
            fs::write(root.join(format!("file-{index}.txt")), []).unwrap();
        }

        let report = inspect(&root).unwrap();
        assert!(report.truncated);
        assert_eq!(report.entries.len(), MAX_ENTRIES);
        assert_eq!(report.counts.total_entries, MAX_ENTRIES);
        assert_eq!(report.counts.regular_files, MAX_ENTRIES);
    }

    #[cfg(unix)]
    #[test]
    fn stops_at_entry_limit_before_descending_pending_directories() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("skill");
        let blocked = root.join("blocked");
        fs::create_dir_all(&blocked).unwrap();
        fs::write(blocked.join("hidden.txt"), []).unwrap();
        for index in 0..MAX_ENTRIES {
            fs::write(root.join(format!("file-{index}.txt")), []).unwrap();
        }
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).unwrap();

        let report = inspect(&root).unwrap();
        assert!(report.truncated);
        assert_eq!(report.entries.len(), MAX_ENTRIES);
        assert!(!report
            .entries
            .iter()
            .any(|entry| entry.path == "blocked/hidden.txt"));
    }
}
