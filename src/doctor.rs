use crate::{config, discovery, index, integration, inventory, output, search};
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
    pub sources: Vec<discovery::SourceSpec>,
    pub diagnostics: Vec<String>,
    pub integration_present: bool,
    pub counts: index::Counts,
    pub required: Vec<Requirement>,
}

#[derive(Serialize)]
pub struct Requirement {
    pub name: String,
    pub resolved_id: Option<String>,
    pub diagnostic: Option<String>,
}

pub fn inspect(config_path: &Path, cwd: &Path, required: &[String]) -> Result<Report, String> {
    let settings = config::load(config_path)?;
    let workspace = config::normalize_cwd(cwd)?;
    let mut diagnostics = Vec::new();
    let reconciled = inventory::reconcile(&settings, &workspace, "doctor");
    let mut counts = index::Counts::default();
    let mut sources = Vec::new();
    let mut requirements = Vec::new();
    match &reconciled {
        Ok(snapshot) => {
            sources = snapshot.sources.clone();
            diagnostics.extend(snapshot.diagnostics.clone());
            counts =
                index::counts(&snapshot.db, Some(&snapshot.roots)).map_err(|e| e.to_string())?;
            for name in required {
                let rows = search::find_name(&snapshot.db, name, Some(&snapshot.roots))
                    .map_err(|e| e.to_string())?;
                let diagnostic = match rows.len() {
                    1 => None,
                    0 => Some("required skill is unavailable in the applicable sources".into()),
                    _ => Some(
                        "required skill name is ambiguous; select or correct the source".into(),
                    ),
                };
                requirements.push(Requirement {
                    name: name.clone(),
                    resolved_id: (rows.len() == 1).then(|| rows[0].id.clone()),
                    diagnostic,
                });
            }
        }
        Err(error) => {
            diagnostics.push(error.to_string());
            requirements = required
                .iter()
                .map(|name| Requirement {
                    name: name.clone(),
                    resolved_id: None,
                    diagnostic: Some(
                        "required skill could not be checked because discovery failed".into(),
                    ),
                })
                .collect();
        }
    }
    let expects_integration = settings
        .agents
        .iter()
        .any(|agent| *agent != config::Agent::None);
    let integration_healthy = match integration::diagnostics(&settings.agents) {
        Ok(problems) => {
            let healthy = problems.is_empty();
            diagnostics.extend(problems);
            healthy
        }
        Err(error) => {
            diagnostics.push(error.to_string());
            false
        }
    };
    let integration_present = expects_integration && integration_healthy;
    let healthy = reconciled.is_ok()
        && requirements
            .iter()
            .all(|requirement| requirement.diagnostic.is_none())
        && integration_healthy
        && (!expects_integration || integration_present);
    Ok(Report {
        version: output::JSON_VERSION,
        healthy,
        config: config_path.display().to_string(),
        cache: config::cache_path().display().to_string(),
        sources,
        diagnostics,
        integration_present,
        counts,
        required: requirements,
    })
}

pub fn text(report: &Report) -> io::Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    writeln!(output, "healthy: {}\nsources: {}\nfilesystem: {}\nraw: {}\nduplicates: {}\nmodel-discoverable: {}\ncache: {}\nintegration: {}",
        report.healthy, report.sources.len(), report.counts.filesystem, report.counts.raw,
        report.counts.duplicates, report.counts.model_discoverable, output::clean(&report.cache), report.integration_present)?;
    writeln!(
        output,
        "groups: {}\nverified-copies: {}",
        report.counts.groups, report.counts.verified_copies
    )?;
    for source in &report.sources {
        writeln!(
            output,
            "source: {}",
            serde_json::to_string(source).expect("serializable source")
        )?;
    }
    for requirement in &report.required {
        writeln!(
            output,
            "required: {}: {}",
            output::clean(&requirement.name),
            output::clean(
                requirement
                    .diagnostic
                    .as_deref()
                    .or(requirement.resolved_id.as_deref())
                    .unwrap_or("unavailable")
            )
        )?;
    }
    for diagnostic in &report.diagnostics {
        writeln!(output, "diagnostic: {}", output::clean(diagnostic))?;
    }
    Ok(())
}
