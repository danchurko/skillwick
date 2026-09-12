use crate::{config, native, sources};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::{
    fs,
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
}

pub fn inspect(config_path: &Path, cwd: &Path) -> Result<Report, String> {
    let settings = config::load(config_path)?;
    let scan = sources::scan(cwd, &settings.roots);
    let mut diagnostics = scan.diagnostics;
    let cache = config::cache_path();
    let cache_readable =
        Connection::open_with_flags(&cache, OpenFlags::SQLITE_OPEN_READ_ONLY).is_ok();
    if !cache_readable {
        diagnostics.push("readable cache unavailable; run skillwick refresh".into());
    }
    let codex = native::detect(&settings).ok();
    let supported = codex
        .as_ref()
        .is_some_and(|codex| native::supports_native_catalog(&codex.version));
    let snapshot = match (&codex, cache_readable) {
        (Some(codex), true) => {
            Connection::open_with_flags(&cache, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .ok()
                .and_then(|db| {
                    crate::index::has_snapshot(
                        &db,
                        cwd,
                        &codex.version,
                        &codex.path,
                        &config::codex_home(&settings),
                    )
                    .ok()
                })
                .unwrap_or(false)
        }
        _ => false,
    };
    let instructions = settings
        .instructions_file
        .clone()
        .unwrap_or_else(|| config::codex_home(&settings).join("AGENTS.md"));
    let integration_present = fs::read_to_string(instructions).is_ok_and(|text| {
        text.matches("<!-- skillwick:begin -->").count() == 1
            && text.matches("<!-- skillwick:end -->").count() == 1
    });
    let native_required = settings.inventory == config::Inventory::Codex;
    let integration_required = settings.agent == config::Agent::Codex;
    let healthy = scan.complete
        && !scan.skills.is_empty()
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
    })
}

pub fn text(report: &Report) -> io::Result<()> {
    let stdout = io::stdout();
    writeln!(stdout.lock(), "healthy: {}\nsources: {}\ncache: {}\nintegration: {}\ncodex: {}\nnative catalogue: {}\nnative snapshot: {}", report.healthy, report.sources, report.cache, report.integration_present, report.codex_version.as_deref().unwrap_or("not found"), report.native_catalog_supported, report.native_snapshot_current)?;
    for diagnostic in &report.diagnostics {
        eprintln!("warning: {diagnostic}");
    }
    Ok(())
}
