use crate::{config, metadata, sources::Skill};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs,
    io::{self, BufRead, BufReader, Read, Write},
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
    let context = config::normalize_context(cwd, codex_home)?;
    let mut child = Command::new(&codex.path)
        .args(["app-server", "--stdio"])
        .env("CODEX_HOME", &context.codex_home)
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
    let parse_context = context.clone();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_bounded_line(&mut reader, MAX_MESSAGE) {
                Ok(None) => {
                    let _ = sender.send(Err("Codex inventory reached EOF".into()));
                    break;
                }
                Ok(Some(line)) => match serde_json::from_slice::<Response>(&line) {
                    Ok(response) if response.id == Some(2) => {
                        let _ = sender.send(parse_inventory(response, &parse_context));
                        break;
                    }
                    Ok(_) => {}
                    Err(error) => {
                        let _ = sender.send(Err(format!("invalid Codex inventory JSON: {error}")));
                        break;
                    }
                },
                Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                    let _ = sender.send(Err("Codex inventory message exceeds 1 MiB".into()));
                    break;
                }
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
        json!({"jsonrpc":"2.0","id":2,"method":"skills/list","params":{"cwds":[context.workspace],"forceReload":true}}),
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

fn read_bounded_line<R: BufRead>(reader: &mut R, limit: usize) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Ok(Some(line))
            };
        }

        let take = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(buffer.len(), |position| position + 1);
        if line.len().saturating_add(take) > limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "line exceeds configured limit",
            ));
        }
        line.extend_from_slice(&buffer[..take]);
        reader.consume(take);
        if take > 0 && line.last() == Some(&b'\n') {
            return Ok(Some(line));
        }
    }
}

fn parse_inventory(response: Response, context: &config::Context) -> Result<Vec<Skill>, String> {
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
            let canonical = validate_native_path(&native.path)?;
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
                workspace: Some(context.workspace.clone()),
                codex_home: Some(context.codex_home.clone()),
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

fn validate_native_path(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("native skill path has no package: {}", path.display()))?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|e| format!("native skill package {}: {e}", parent.display()))?;
    let canonical =
        fs::canonicalize(path).map_err(|e| format!("native skill path {}: {e}", path.display()))?;
    if !canonical.is_file() {
        return Err(format!(
            "native skill path is not a file: {}",
            path.display()
        ));
    }
    if canonical.file_name().and_then(|name| name.to_str()) != Some("SKILL.md") {
        return Err(format!(
            "native skill path is not SKILL.md: {}",
            path.display()
        ));
    }
    if !canonical.starts_with(&canonical_parent) {
        return Err(format!(
            "native skill path escapes package: {}",
            path.display()
        ));
    }
    Ok(canonical)
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
    use std::io::Cursor;

    #[test]
    fn compatibility_is_explicit() {
        assert!(supports_native_catalog("0.154.0"));
        assert!(!supports_native_catalog("0.155.0"));
    }

    #[test]
    fn protocol_lines_are_bounded_before_json_parsing() {
        let mut reader = BufReader::new(Cursor::new(b"{}\n".to_vec()));
        assert_eq!(
            read_bounded_line(&mut reader, MAX_MESSAGE).unwrap(),
            Some(b"{}\n".to_vec())
        );

        let mut reader = BufReader::new(Cursor::new(vec![b'x'; MAX_MESSAGE + 1]));
        let error = read_bounded_line(&mut reader, MAX_MESSAGE).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[cfg(unix)]
    #[test]
    fn native_path_rejects_a_symlink_escape() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let package = temp.path().join("package");
        let outside = temp.path().join("outside");
        fs::create_dir_all(&package).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("SKILL.md"), "---\nname: outside\n---\n").unwrap();
        symlink(outside.join("SKILL.md"), package.join("SKILL.md")).unwrap();

        let error = validate_native_path(&package.join("SKILL.md")).unwrap_err();
        assert!(error.contains("escapes package"));
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
