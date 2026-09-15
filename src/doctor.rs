use crate::{config, index, integration, inventory, sources};
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
    pub integration_present: bool,
    pub counts: index::Counts,
}

pub fn inspect(config_path: &Path, cwd: &Path) -> Result<Report, String> {
    let settings = config::load(config_path)?;
    let workspace = config::normalize_cwd(cwd)?;
    let mut diagnostics = Vec::new();
    let cache = crate::config::cache_path();
    let reconciled = inventory::reconcile(&settings, &workspace, "doctor");
    if let Err(error) = &reconciled {
        diagnostics.push(error.to_string());
    }
    let roots = sources::root_keys(&workspace, &settings.roots, &settings.projects);
    if let Ok(db) = &reconciled {
        for diagnostic in crate::index::policy_diagnostics(db).unwrap_or_default() {
            if !diagnostics.contains(&diagnostic) {
                diagnostics.push(diagnostic);
            }
        }
    }
    let counts = reconciled
        .as_ref()
        .ok()
        .and_then(|db| crate::index::counts(db, Some(&roots)).ok())
        .unwrap_or_default();
    let sources = counts.filesystem;
    let instructions = settings
        .instructions_file
        .clone()
        .unwrap_or_else(|| config::codex_home().join("AGENTS.md"));
    let integration_present =
        integration::integration_present(&instructions, &config::codex_home());
    let inventory_ready = reconciled.is_ok();
    let healthy =
        inventory_ready && (settings.agent != config::Agent::Codex || integration_present);
    Ok(Report {
        version: 2,
        healthy,
        config: config_path.display().to_string(),
        cache: cache.display().to_string(),
        sources,
        diagnostics,
        integration_present,
        counts,
    })
}

pub fn text(report: &Report) -> io::Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(
        output,
        "healthy: {}\nsources: {}\nfilesystem: {}\nraw: {}\nduplicates: {}\nmodel-discoverable: {}\ncache: {}\nintegration: {}",
        report.healthy,
        report.sources,
        report.counts.filesystem,
        report.counts.raw,
        report.counts.duplicates,
        report.counts.model_discoverable,
        report.cache,
        report.integration_present,
    )?;
    for diagnostic in &report.diagnostics {
        eprintln!("warning: {diagnostic}");
    }
    Ok(())
}
