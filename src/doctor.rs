use crate::{config, index, integration, native, sources};
use rusqlite::backup::Backup;
use serde::Serialize;
use std::{
    io::{self, Write},
    path::Path,
    time::Duration,
};

#[derive(Serialize)]
pub struct Report {
    pub version: u8,
    pub healthy: bool,
    pub config: String,
    pub cache: String,
    pub sources: usize,
    pub diagnostics: Vec<String>,
    pub codex_version: Option<String>,
    pub native_catalog_supported: bool,
    pub native_snapshot_current: bool,
    pub integration_present: bool,
    pub counts: index::Counts,
}

pub fn inspect(config_path: &Path, cwd: &Path) -> Result<Report, String> {
    let settings = config::load(config_path)?;
    let context = if settings.inventory == config::Inventory::Codex {
        Some(config::normalize_context(
            cwd,
            &config::codex_home(&settings),
        )?)
    } else {
        None
    };
    let workspace = context
        .as_ref()
        .map_or(cwd, |context| context.workspace.as_path());
    let scan = sources::scan(workspace, &settings.roots);
    let mut diagnostics = scan.diagnostics;
    let cache = config::cache_path();
    let cache_db = crate::index::open_read_only(&cache).ok();
    let cache_readable = cache_db.is_some();
    if !cache_readable {
        diagnostics.push("readable cache unavailable; run skillwick refresh".into());
    }
    let diagnostic_db = diagnostic_database(cache_db.as_ref(), &scan.skills, scan.complete);
    if let Some(db) = &diagnostic_db {
        for diagnostic in crate::index::policy_diagnostics(db, context.as_ref()).unwrap_or_default()
        {
            if !diagnostics.contains(&diagnostic) {
                diagnostics.push(diagnostic);
            }
        }
    }
    let mut counts = diagnostic_db
        .as_ref()
        .and_then(|db| crate::index::counts(db, context.as_ref()).ok())
        .unwrap_or_default();
    if diagnostic_db.is_none() {
        counts.filesystem = scan.skills.len();
        counts.raw = counts.filesystem;
        counts.model_discoverable = scan
            .skills
            .iter()
            .filter(|skill| skill.metadata.invocation_policy.model_discoverable())
            .count();
    }
    let codex = native::detect(&settings).ok();
    let supported = codex
        .as_ref()
        .is_some_and(|codex| native::supports_native_catalog(&codex.version));
    let snapshot = match (&codex, cache_readable, &context) {
        (Some(codex), true, Some(context)) => cache_db
            .as_ref()
            .and_then(|db| {
                crate::index::has_snapshot(
                    db,
                    &context.workspace,
                    &codex.version,
                    &codex.path,
                    &context.codex_home,
                )
                .ok()
            })
            .unwrap_or(false),
        _ => false,
    };
    let instructions = settings
        .instructions_file
        .clone()
        .unwrap_or_else(|| config::codex_home(&settings).join("AGENTS.md"));
    let integration_present =
        integration::integration_present(&instructions, &config::codex_home(&settings));
    let native_required = settings.inventory == config::Inventory::Codex;
    let integration_required = settings.agent == config::Agent::Codex;
    let inventory_ready = if native_required {
        cache_db.as_ref().is_some_and(|db| {
            crate::index::has_kind_for_context(db, "codex", context.as_ref()).unwrap_or(false)
        })
    } else {
        scan.complete && !scan.skills.is_empty()
    };
    let healthy = inventory_ready
        && cache_readable
        && (!native_required || supported && snapshot)
        && (!integration_required || integration_present);
    Ok(Report {
        version: 2,
        healthy,
        config: config_path.display().to_string(),
        cache: cache.display().to_string(),
        sources: scan.skills.len(),
        diagnostics,
        codex_version: codex.map(|codex| codex.version),
        native_catalog_supported: supported,
        native_snapshot_current: snapshot,
        integration_present,
        counts,
    })
}

fn diagnostic_database(
    cache_db: Option<&rusqlite::Connection>,
    skills: &[sources::Skill],
    complete: bool,
) -> Option<rusqlite::Connection> {
    let mut database = index::open(Path::new(":memory:")).ok()?;
    if let Some(source) = cache_db {
        {
            let backup = Backup::new(source, &mut database).ok()?;
            backup.run_to_completion(100, Duration::ZERO, None).ok()?;
        }
    }
    index::refresh_kind(&mut database, "filesystem", skills, complete).ok()?;
    Some(database)
}

pub fn text(report: &Report) -> io::Result<()> {
    let stdout = io::stdout();
    writeln!(stdout.lock(), "healthy: {}\nsources: {}\nfilesystem: {}\nnative: {}\nraw: {}\nduplicates: {}\nmodel-discoverable: {}\ncache: {}\nintegration: {}\ncodex: {}\nnative catalogue: {}\nnative snapshot: {}", report.healthy, report.sources, report.counts.filesystem, report.counts.native, report.counts.raw, report.counts.duplicates, report.counts.model_discoverable, report.cache, report.integration_present, report.codex_version.as_deref().unwrap_or("not found"), report.native_catalog_supported, report.native_snapshot_current)?;
    for diagnostic in &report.diagnostics {
        eprintln!("warning: {diagnostic}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{index, sources};
    use std::{env, fs};

    fn write_skill(workspace: &std::path::Path, name: &str, extra: &str) {
        let directory = workspace.join(".agents/skills").join(name);
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {name} skill\n{extra}---\n"),
        )
        .unwrap();
    }

    #[test]
    fn reports_current_workspace_counts_and_diagnostics_without_publishing() {
        let temporary = tempfile::tempdir().unwrap();
        let workspace_a = temporary.path().join("workspace-a");
        let workspace_b = temporary.path().join("workspace-b");
        let home = temporary.path().join("home");
        fs::create_dir_all(&workspace_a).unwrap();
        fs::create_dir_all(&workspace_b).unwrap();
        fs::create_dir_all(&home).unwrap();
        write_skill(
            &workspace_a,
            "from-a-policy",
            "disable-model-invocation: maybe\n",
        );
        write_skill(&workspace_a, "from-a-extra", "");
        write_skill(&workspace_b, "from-b", "");

        let cache_home = temporary.path().join("cache");
        let previous_home = env::var_os("HOME");
        let previous_cache_home = env::var_os("XDG_CACHE_HOME");
        env::set_var("HOME", &home);
        env::set_var("XDG_CACHE_HOME", &cache_home);
        let scan_a = sources::scan(&workspace_a, &[]);
        let scan_b = sources::scan(&workspace_b, &[]);
        assert!(scan_a.complete);
        assert!(scan_b.complete);
        assert!(scan_a.skills.len() > scan_b.skills.len());

        let cache = config::cache_path();
        let mut database = index::open(std::path::Path::new(":memory:")).unwrap();
        index::refresh_kind(&mut database, "filesystem", &scan_a.skills, scan_a.complete).unwrap();
        index::publish(&database, &cache).unwrap();
        let before = fs::read(&cache).unwrap();

        let result = inspect(&temporary.path().join("config.toml"), &workspace_b);

        match previous_home {
            Some(value) => env::set_var("HOME", value),
            None => env::remove_var("HOME"),
        }
        match previous_cache_home {
            Some(value) => env::set_var("XDG_CACHE_HOME", value),
            None => env::remove_var("XDG_CACHE_HOME"),
        }

        let report = result.unwrap();
        assert_eq!(report.sources, scan_b.skills.len());
        assert_eq!(report.counts.filesystem, scan_b.skills.len());
        assert_eq!(report.counts.raw, scan_b.skills.len());
        assert_eq!(
            report.counts.model_discoverable,
            scan_b
                .skills
                .iter()
                .filter(|skill| skill.metadata.invocation_policy.model_discoverable())
                .count()
        );
        assert!(!report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("from-a-policy")));
        assert_eq!(before, fs::read(&cache).unwrap());
    }
}
