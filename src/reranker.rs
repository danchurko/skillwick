//! Optional setup-prepared reranking. Inventory and result identity remain local.
use crate::{
    config::{self, Reranker, RerankerBackend},
    search::{self, ResultRow},
};
use rusqlite::{Connection, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const PROGRAM: &str = include_str!("../assets/skillwick/reranker_runtime.py");
const JEV_PACKAGES: &[&str] = &["typesafe-sdk==0.7.2"];
const BERT_PACKAGES: &[&str] = &[
    "numpy==2.5.3",
    "onnxruntime==1.30.0",
    "tokenizers==0.23.2",
    "huggingface-hub==1.33.0",
];
const MAX_OUTPUT: u64 = 65_536;
const SEARCH_TIMEOUT: Duration = Duration::from_secs(25);
const PREPARATION_LOCK_TIMEOUT: Duration = Duration::from_secs(30);

pub struct SearchOutcome {
    pub rows: Vec<ResultRow>,
    pub diagnostic: Option<&'static str>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Ready {
    backend: RerankerBackend,
    program_sha256: String,
    packages: Vec<String>,
}

fn packages(backend: RerankerBackend) -> &'static [&'static str] {
    match backend {
        RerankerBackend::None => &[],
        RerankerBackend::Tinybert => BERT_PACKAGES,
        RerankerBackend::Jev => JEV_PACKAGES,
    }
}

fn expected_ready(backend: RerankerBackend) -> Ready {
    Ready {
        backend,
        program_sha256: format!("{:x}", Sha256::digest(PROGRAM.as_bytes())),
        packages: packages(backend).iter().map(|v| (*v).to_owned()).collect(),
    }
}

/// Plan only: never downloads, creates files, or authenticates.
pub fn planned(backend: RerankerBackend) -> Reranker {
    let expected = expected_ready(backend);
    let hash = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&expected).expect("serializable runtime manifest"))
    );
    static GENERATION: AtomicU64 = AtomicU64::new(0);
    let suffix = if backend == RerankerBackend::Jev {
        format!(
            "-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos(),
            GENERATION.fetch_add(1, Ordering::Relaxed)
        )
    } else {
        String::new()
    };
    Reranker {
        backend,
        runtime: (backend != RerankerBackend::None).then(|| {
            config::state_dir().join("rerankers").join(format!(
                "{}-{}{}",
                backend.as_str(),
                &hash[..16],
                suffix
            ))
        }),
    }
}

fn python(directory: &Path) -> PathBuf {
    directory.join("venv").join(if cfg!(windows) {
        "Scripts/python.exe"
    } else {
        "bin/python"
    })
}

fn nonsymlink_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|v| v.is_file() && !v.file_type().is_symlink())
}

fn ready(directory: &Path, backend: RerankerBackend) -> bool {
    let script = directory.join("backend.py");
    let manifest = directory.join("ready.json");
    nonsymlink_file(&script)
        && nonsymlink_file(&manifest)
        && fs::read(&script).is_ok_and(|v| v == PROGRAM.as_bytes())
        && fs::read(&manifest)
            .ok()
            .and_then(|v| serde_json::from_slice::<Ready>(&v).ok())
            .is_some_and(|v| v == expected_ready(backend))
        && python(directory).is_file()
}

fn private_directory(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok_and(|v| v.file_type().is_symlink()) {
        return Err("reranker runtime directory is a symlink".into());
    }
    fs::create_dir_all(path).map_err(|_| "cannot create reranker runtime directory")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| "cannot protect reranker runtime directory")?;
    }
    Ok(())
}

fn credential(directory: &Path) -> Result<String, &'static str> {
    if let Ok(key) = std::env::var("TYPESAFE_API_KEY") {
        if !key.trim().is_empty() {
            return Ok(key.trim().to_owned());
        }
    }
    let path = directory.join("api-key");
    if !nonsymlink_file(&path) {
        return Err("missing_api_key");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(&path)
            .map_err(|_| "missing_api_key")?
            .permissions()
            .mode()
            & 0o077
            != 0
        {
            return Err("unsafe_credential_permissions");
        }
    }
    let key = fs::read_to_string(path).map_err(|_| "missing_api_key")?;
    if key.trim().is_empty() {
        return Err("missing_api_key");
    }
    Ok(key.trim().to_owned())
}

/// Drain bounded stdout, suppress raw errors, and enforce a wall-clock deadline.
fn execute(
    mut command: Command,
    input: Vec<u8>,
    timeout: Duration,
) -> Result<Vec<u8>, &'static str> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let started = Instant::now();
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "runtime_unavailable")?;
    let mut stdin = child.stdin.take().ok_or("runtime_unavailable")?;
    let stdout = child.stdout.take().ok_or("runtime_unavailable")?;
    let (write_send, write_receive) = mpsc::channel();
    let (read_send, read_receive) = mpsc::channel();
    thread::spawn(move || {
        let _ = write_send.send(stdin.write_all(&input));
    });
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take(MAX_OUTPUT + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = read_send.send(result);
    });
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(10)),
            _ => {
                break Err("runtime_timeout");
            }
        }
    };
    // Descendants may retain inherited pipes even after the direct child exits.
    #[cfg(unix)]
    {
        let _ = Command::new("/bin/kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    if status.is_err() {
        let _ = child.kill();
        thread::spawn(move || {
            let _ = child.wait();
        });
    }
    let status = status?;
    write_receive
        .recv_timeout(timeout.saturating_sub(started.elapsed()))
        .map_err(|_| "runtime_timeout")?
        .map_err(|_| "runtime_input_invalid")?;
    let bytes = read_receive
        .recv_timeout(timeout.saturating_sub(started.elapsed()))
        .map_err(|_| "runtime_timeout")?
        .map_err(|_| "runtime_output_invalid")?;
    if bytes.len() as u64 > MAX_OUTPUT {
        return Err("runtime_output_invalid");
    }
    if !status.success() {
        let error = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|v| v.get("error").and_then(|v| v.as_str()).map(str::to_owned));
        return Err(match error.as_deref() {
            Some("authentication") => "authentication",
            Some("missing_api_key") => "missing_api_key",
            Some("rate_limit") => "rate_limit",
            Some("timeout") => "timeout",
            Some("model_mismatch") => "model_mismatch",
            Some("malformed_response") => "malformed_response",
            Some("artifact_mismatch") => "artifact_mismatch",
            Some("transport_error") => "transport_error",
            Some("server_error") => "server_error",
            _ => "backend_failure",
        });
    }
    Ok(bytes)
}

fn runtime_command(directory: &Path) -> Command {
    let mut command = Command::new(python(directory));
    command.arg("-I").arg(directory.join("backend.py"));
    command
        .env("TYPESAFE_LOG_LEVEL", "off")
        .env("HF_HUB_DISABLE_TELEMETRY", "1")
        .env("TOKENIZERS_PARALLELISM", "false");
    command
}

/// Prepare a versioned runtime, and publish its readiness only after validation.
/// A failed preparation never changes the caller's configuration.
pub fn prepare(settings: &Reranker, previous: Option<&Reranker>) -> Result<(), String> {
    if settings.backend == RerankerBackend::None {
        return Ok(());
    }
    let directory = settings
        .runtime
        .as_deref()
        .filter(|v| v.is_absolute())
        .ok_or("invalid reranker runtime directory")?;
    let key = if settings.backend == RerankerBackend::Jev {
        Some(
            credential(directory)
                .or_else(|error| {
                    previous
                        .and_then(|v| v.runtime.as_deref())
                        .map(credential)
                        .unwrap_or(Err(error))
                })
                .map_err(|v| format!("reranker preparation failed ({v})"))?,
        )
    } else {
        None
    };
    private_directory(directory)?;
    // TinyBERT reuses a deterministic directory. Serialize preparation there
    // without holding the global config publication lock.
    let lock_path = directory.join("preparation.sqlite");
    config::refuse_symlink(&lock_path)?;
    let mut lock_db = Connection::open(&lock_path)
        .map_err(|error| format!("cannot open reranker preparation lock: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("cannot protect reranker preparation lock: {error}"))?;
    }
    lock_db
        .busy_timeout(PREPARATION_LOCK_TIMEOUT)
        .map_err(|error| format!("cannot configure reranker preparation timeout: {error}"))?;
    let transaction = lock_db
        .transaction_with_behavior(TransactionBehavior::Exclusive)
        .map_err(|error| format!("cannot lock reranker preparation: {error}"))?;
    if !ready(directory, settings.backend) {
        let mut venv = Command::new("uv");
        venv.args(["venv", "--allow-existing", "--python", "3.14"])
            .arg(directory.join("venv"))
            .env_remove("TYPESAFE_API_KEY");
        execute(venv, Vec::new(), Duration::from_secs(180))
            .map_err(|v| format!("reranker Python preparation failed ({v})"))?;
        let mut install = Command::new("uv");
        install
            .args(["pip", "install", "--python"])
            .arg(python(directory))
            .args(packages(settings.backend))
            .env_remove("TYPESAFE_API_KEY");
        execute(install, Vec::new(), Duration::from_secs(300))
            .map_err(|v| format!("reranker dependency installation failed ({v})"))?;
        config::atomic_write(&directory.join("backend.py"), PROGRAM.as_bytes(), 0o600)?;
    }
    let mut command = runtime_command(directory);
    command
        .arg("--prepare")
        .arg(settings.backend.as_str())
        .arg("--cache")
        .arg(directory.join("models"));
    if let Some(key) = &key {
        command.env("TYPESAFE_API_KEY", key);
    } else {
        command.env_remove("TYPESAFE_API_KEY");
    }
    let response = execute(command, Vec::new(), Duration::from_secs(180))
        .map_err(|v| format!("reranker preparation failed ({v})"))?;
    let response: serde_json::Value =
        serde_json::from_slice(&response).map_err(|_| "reranker preparation response invalid")?;
    let model = match settings.backend {
        RerankerBackend::Jev => "jev-1.13.0",
        RerankerBackend::Tinybert => {
            "cross-encoder/ms-marco-TinyBERT-L2-v2@81d1926f67cb8eee2c2be17ca9f793c7c3bd20cc"
        }
        RerankerBackend::None => unreachable!(),
    };
    if response.get("status").and_then(|v| v.as_str()) != Some("ready")
        || response.get("backend").and_then(|v| v.as_str()) != Some(settings.backend.as_str())
        || response.get("model").and_then(|v| v.as_str()) != Some(model)
    {
        return Err("reranker preparation response invalid".into());
    }
    config::atomic_write(
        &directory.join("ready.json"),
        &serde_json::to_vec(&expected_ready(settings.backend)).expect("serializable readiness"),
        0o600,
    )?;
    if let Some(key) = key {
        config::atomic_write(&directory.join("api-key"), key.as_bytes(), 0o600)?;
    }
    transaction
        .commit()
        .map_err(|error| format!("cannot finish reranker preparation: {error}"))?;
    Ok(())
}

#[derive(Serialize)]
struct Candidate<'a> {
    id: String,
    name: &'a str,
    description: &'a str,
}
#[derive(Serialize)]
struct Request<'a> {
    backend: &'static str,
    query: &'a str,
    candidates: Vec<Candidate<'a>>,
    cache: PathBuf,
}
#[derive(Deserialize)]
struct Ranking {
    ranked: Vec<String>,
    model: String,
}

fn permutation(
    bytes: &[u8],
    count: usize,
    backend: RerankerBackend,
) -> Result<Vec<usize>, &'static str> {
    let value: Ranking = serde_json::from_slice(bytes).map_err(|_| "runtime_output_invalid")?;
    let expected_model = match backend {
        RerankerBackend::Jev => "jev-1.13.0",
        RerankerBackend::Tinybert => {
            "cross-encoder/ms-marco-TinyBERT-L2-v2@81d1926f67cb8eee2c2be17ca9f793c7c3bd20cc"
        }
        RerankerBackend::None => return Err("runtime_output_invalid"),
    };
    if value.model != expected_model {
        return Err("model_mismatch");
    }
    if value.ranked.len() != count {
        return Err("candidate_mapping_invalid");
    }
    let expected: Vec<_> = (0..count).map(|v| format!("c{v:03}")).collect();
    let mut seen = HashSet::new();
    value
        .ranked
        .iter()
        .map(|label| {
            let index = expected
                .iter()
                .position(|v| v == label)
                .ok_or("candidate_mapping_invalid")?;
            if !seen.insert(index) {
                return Err("candidate_mapping_invalid");
            }
            Ok(index)
        })
        .collect()
}

fn ordering(
    query: &str,
    rows: &[ResultRow],
    settings: &Reranker,
) -> Result<Vec<usize>, &'static str> {
    let directory = settings.runtime.as_deref().ok_or("runtime_unavailable")?;
    if !ready(directory, settings.backend) {
        return Err("runtime_unavailable");
    }
    let key = if settings.backend == RerankerBackend::Jev {
        Some(credential(directory)?)
    } else {
        None
    };
    if key.as_ref().is_some_and(|key| {
        query.contains(key)
            || rows
                .iter()
                .any(|row| row.name.contains(key) || row.description.contains(key))
    }) {
        return Err("secret_in_payload");
    }
    let request = Request {
        backend: settings.backend.as_str(),
        query,
        candidates: rows
            .iter()
            .enumerate()
            .map(|(i, row)| Candidate {
                id: format!("c{i:03}"),
                name: &row.name,
                description: &row.description,
            })
            .collect(),
        cache: directory.join("models"),
    };
    let input = serde_json::to_vec(&request).map_err(|_| "runtime_input_invalid")?;
    if input.len() > 131_072 {
        return Err("runtime_input_too_large");
    }
    let mut command = runtime_command(directory);
    if let Some(key) = key {
        command.env("TYPESAFE_API_KEY", key);
    } else {
        command.env_remove("TYPESAFE_API_KEY");
    }
    permutation(
        &execute(command, input, SEARCH_TIMEOUT)?,
        rows.len(),
        settings.backend,
    )
}

/// Configured library search: reconcile inventory before calling this boundary.
/// Reranker failures retain lexical order; database/freshness failures are not masked.
pub fn search(
    db: &Connection,
    query: &str,
    limit: usize,
    roots: Option<&[String]>,
    settings: &Reranker,
) -> rusqlite::Result<SearchOutcome> {
    if !(1..=20).contains(&limit) {
        return Err(rusqlite::Error::InvalidParameterName(
            "search limit must be 1–20".into(),
        ));
    }
    let enabled = settings.backend != RerankerBackend::None;
    let mut rows = search::query(db, query, if enabled { 20 } else { limit }, roots)?;
    let diagnostic = if enabled && !rows.is_empty() {
        match ordering(query, &rows, settings) {
            Ok(order) => {
                rows = order.into_iter().map(|i| rows[i].clone()).collect();
                None
            }
            Err(category) => Some(category),
        }
    } else {
        None
    };
    rows.truncate(limit);
    Ok(SearchOutcome { rows, diagnostic })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_complete_bounded_mapping_and_model() {
        let good = br#"{"ranked":["c001","c000"],"model":"jev-1.13.0"}"#;
        assert_eq!(
            permutation(good, 2, RerankerBackend::Jev).unwrap(),
            vec![1, 0]
        );
        for bad in [
            br#"{"ranked":["c000","c000"],"model":"jev-1.13.0"}"#.as_slice(),
            br#"{"ranked":["c999","c000"],"model":"jev-1.13.0"}"#,
            br#"{"ranked":["c000"],"model":"jev-1.13.0"}"#,
        ] {
            assert_eq!(
                permutation(bad, 2, RerankerBackend::Jev),
                Err("candidate_mapping_invalid")
            );
        }
        assert_eq!(
            permutation(good, 2, RerankerBackend::Tinybert),
            Err("model_mismatch")
        );
    }
    #[test]
    fn preparations_have_independent_credentials() {
        let first = planned(RerankerBackend::Jev);
        let second = planned(RerankerBackend::Jev);
        assert_ne!(first.runtime, second.runtime);
        assert_eq!(
            planned(RerankerBackend::Tinybert),
            planned(RerankerBackend::Tinybert)
        );
        let temporary = tempfile::tempdir().unwrap();
        let old = temporary.path().join("old");
        let new = temporary.path().join("new");
        private_directory(&old).unwrap();
        private_directory(&new).unwrap();
        config::atomic_write(&old.join("api-key"), b"old-key", 0o600).unwrap();
        config::atomic_write(&new.join("api-key"), b"new-key", 0o600).unwrap();
        assert_eq!(fs::read_to_string(old.join("api-key")).unwrap(), "old-key");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(new.join("api-key"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn descendants_cannot_hold_stdout_open_after_parent_exit() {
        let mut command = Command::new("python3");
        command.args(["-c", "import subprocess; subprocess.Popen(['python3','-c','import time; time.sleep(10)']); print('{}')"]);
        let started = Instant::now();
        assert_eq!(
            execute(command, Vec::new(), Duration::from_secs(2)).unwrap(),
            b"{}\n"
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn subprocess_timeout_and_output_are_bounded() {
        let mut command = Command::new("python3");
        command.args(["-c", "import time;time.sleep(10)"]);
        assert_eq!(
            execute(command, Vec::new(), Duration::from_millis(30)),
            Err("runtime_timeout")
        );
        let mut command = Command::new("python3");
        command.args(["-c", "print('x'*70000)"]);
        assert_eq!(
            execute(command, Vec::new(), Duration::from_secs(5)),
            Err("runtime_output_invalid")
        );
    }
}
