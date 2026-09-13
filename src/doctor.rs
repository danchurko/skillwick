use crate::{config, index, integration, native, sources};
use serde::Serialize;
use std::{
    io::{self, Write},
    path::Path,
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
    if let Some(db) = &cache_db {
        for diagnostic in crate::index::policy_diagnostics(db, context.as_ref()).unwrap_or_default()
        {
            if !diagnostics.contains(&diagnostic) {
                diagnostics.push(diagnostic);
            }
        }
    }
    let mut counts = cache_db
        .as_ref()
        .and_then(|db| crate::index::counts(db, context.as_ref()).ok())
        .unwrap_or_default();
    if !cache_readable {
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
        cache_db
            .as_ref()
            .is_some_and(|db| crate::index::has_kind(db, "codex").unwrap_or(false))
    } else {
        scan.complete && !scan.skills.is_empty()
    };
    let healthy = inventory_ready
        && cache_readable
        && (!native_required || supported && snapshot)
        && (!integration_required || integration_present);
    Ok(Report {
        version: 1,
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

pub fn text(report: &Report) -> io::Result<()> {
    let stdout = io::stdout();
    writeln!(stdout.lock(), "healthy: {}\nsources: {}\nfilesystem: {}\nnative: {}\nraw: {}\nduplicates: {}\nmodel-discoverable: {}\ncache: {}\nintegration: {}\ncodex: {}\nnative catalogue: {}\nnative snapshot: {}", report.healthy, report.sources, report.counts.filesystem, report.counts.native, report.counts.raw, report.counts.duplicates, report.counts.model_discoverable, report.cache, report.integration_present, report.codex_version.as_deref().unwrap_or("not found"), report.native_catalog_supported, report.native_snapshot_current)?;
    for diagnostic in &report.diagnostics {
        eprintln!("warning: {diagnostic}");
    }
    Ok(())
}
