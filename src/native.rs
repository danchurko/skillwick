use crate::{config, metadata, sources::Skill};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

const TIMEOUT: Duration = Duration::from_secs(8);
const MAX_MESSAGE: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Codex {
    pub path: PathBuf,
    pub version: String,
}

#[derive(Deserialize)]
struct Response {
    id: Option<u64>,
    result: Option<Value>,
    error: Option<RpcError>,
}
#[derive(Deserialize)]
struct RpcError {
    message: String,
}
#[derive(Deserialize)]
struct SkillsResponse {
    data: Vec<CwdSkills>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CwdSkills {
    skills: Vec<NativeSkill>,
    errors: Vec<Value>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeSkill {
    name: String,
    description: String,
    path: PathBuf,
    scope: String,
    enabled: bool,
    plugin_id: Option<String>,
    short_description: Option<String>,
    interface: Option<NativeInterface>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeInterface {
    short_description: Option<String>,
}

pub fn detect(config: &config::Config) -> Result<Codex, String> {
    let path = match &config.codex_bin {
        Some(path) => path.clone(),
        None => find_in_path("codex").ok_or("Codex executable not found")?,
    };
    let path = fs::canonicalize(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let output = Command::new(&path)
        .arg("--version")
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("{} --version failed", path.display()));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout
        .split_whitespace()
        .last()
        .ok_or("Codex version output was empty")?
        .to_owned();
    Ok(Codex { path, version })
}

pub fn supports_native_catalog(version: &str) -> bool {
    version == "0.154.0"
}

pub fn inventory(codex: &Codex, codex_home: &Path, cwd: &Path) -> Result<Vec<Skill>, String> {
    let mut child = Command::new(&codex.path)
        .args(["app-server", "--stdio"])
        .env("CODEX_HOME", codex_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to start Codex inventory: {e}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("Codex inventory stdout unavailable")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Codex inventory stderr unavailable")?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    let _ = sender.send(Err("Codex inventory reached EOF".into()));
                    break;
                }
                Ok(_) if line.len() > MAX_MESSAGE => {
                    let _ = sender.send(Err("Codex inventory message exceeds 1 MiB".into()));
                    break;
                }
                Ok(_) => match serde_json::from_str::<Response>(&line) {
                    Ok(response) if response.id == Some(2) => {
                        let _ = sender.send(parse_inventory(response));
                        break;
                    }
                    Ok(_) => {}
                    Err(error) => {
                        let _ = sender.send(Err(format!("invalid Codex inventory JSON: {error}")));
                        break;
                    }
                },
                Err(error) => {
                    let _ = sender.send(Err(error.to_string()));
                    break;
                }
            }
        }
    });
    let stderr_reader = thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.take(8192).read_to_string(&mut text);
        text
    });
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"skillwick","title":"Skillwick","version":env!("CARGO_PKG_VERSION")},"capabilities":{}}}),
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
        json!({"jsonrpc":"2.0","id":2,"method":"skills/list","params":{"cwds":[cwd],"forceReload":true}}),
    ];
    let write_result = (|| -> Result<(), String> {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or("Codex inventory stdin unavailable")?;
        for request in requests {
            writeln!(stdin, "{request}").map_err(|error| error.to_string())?;
        }
        stdin.flush().map_err(|error| error.to_string())
    })();
    if let Err(error) = write_result {
        terminate(&mut child);
        return Err(error);
    }
    let response = receiver
        .recv_timeout(TIMEOUT)
        .map_err(|_| "Codex inventory timed out".to_string());
    terminate(&mut child);
    let stderr = stderr_reader.join().unwrap_or_default();
    response.map_err(|error| {
        if stderr.trim().is_empty() {
            error
        } else {
            format!("{error}; stderr: {}", stderr.trim())
        }
    })?
}

fn parse_inventory(response: Response) -> Result<Vec<Skill>, String> {
    if let Some(error) = response.error {
        return Err(format!("Codex inventory failed: {}", error.message));
    }
    let parsed: SkillsResponse = serde_json::from_value(
        response
            .result
            .ok_or("Codex inventory response omitted result")?,
    )
    .map_err(|e| e.to_string())?;
    let mut output = Vec::new();
    for cwd in parsed.data {
        if !cwd.errors.is_empty() {
            return Err(format!(
                "Codex inventory reported {} source error(s)",
                cwd.errors.len()
            ));
        }
        for native in cwd.skills {
            let canonical = fs::canonicalize(&native.path)
                .map_err(|e| format!("{}: {e}", native.path.display()))?;
            let mut parsed = metadata::parse(&canonical)?;
            parsed.name = native.name;
            parsed.description = native
                .interface
                .and_then(|interface| interface.short_description)
                .or(native.short_description)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(native.description);
            if let Some(diagnostic) = &parsed.policy_diagnostic {
                eprintln!("warning: {}: {diagnostic}", native.path.display());
            }
            output.push(Skill {
                base: native.path.parent().unwrap_or(Path::new(".")).to_path_buf(),
                path: native.path,
                canonical,
                scope: native.scope,
                source: format!("codex:{}", native.plugin_id.as_deref().unwrap_or("native")),
                source_kind: "codex".into(),
                enabled: native.enabled,
                plugin_id: native.plugin_id,
                metadata: parsed,
            });
        }
    }
    Ok(output)
}

fn terminate(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}
fn find_in_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compatibility_is_explicit() {
        assert!(supports_native_catalog("0.154.0"));
        assert!(!supports_native_catalog("0.155.0"));
    }
    #[test]
    fn parses_interleaved_release_fixture() {
        let line = include_str!("../tests/fixtures/codex/0.154.0/skills-list.jsonl")
            .lines()
            .find(|line| line.contains("\"id\":2"))
            .unwrap();
        let response: Response = serde_json::from_str(line).unwrap();
        let parsed: SkillsResponse = serde_json::from_value(response.result.unwrap()).unwrap();
        assert_eq!(parsed.data[0].skills.len(), 2);
        assert!(!parsed.data[0].skills[1].enabled);
        assert_eq!(
            parsed.data[0].skills[1].plugin_id.as_deref(),
            Some("fixture")
        );
    }
}
