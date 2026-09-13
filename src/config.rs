use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub workspace: PathBuf,
    pub codex_home: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub roots: Vec<PathBuf>,
    pub inventory: Inventory,
    pub agent: Agent,
    pub codex_home: Option<PathBuf>,
    pub codex_bin: Option<PathBuf>,
    pub instructions_file: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Inventory {
    #[default]
    Filesystem,
    Codex,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    #[default]
    None,
    Codex,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            inventory: Inventory::Filesystem,
            agent: Agent::None,
            codex_home: None,
            codex_bin: None,
            instructions_file: None,
        }
    }
}
pub fn default_path() -> PathBuf {
    config_home().join("skillwick/config.toml")
}
pub fn cache_path() -> PathBuf {
    env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".cache"))
        .join("skillwick/index-v3.sqlite")
}
pub fn state_dir() -> PathBuf {
    env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/state"))
        .join("skillwick")
}
pub fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
pub fn codex_home(config: &Config) -> PathBuf {
    config
        .codex_home
        .clone()
        .or_else(|| env::var_os("CODEX_HOME").map(PathBuf::from))
        .unwrap_or_else(|| home().join(".codex"))
}

pub fn normalize_context(cwd: &Path, codex_home: &Path) -> Result<Context, String> {
    let workspace = fs::canonicalize(cwd)
        .map_err(|error| format!("cannot normalize workspace {}: {error}", cwd.display()))?;
    let codex_home = fs::canonicalize(codex_home).map_err(|error| {
        format!(
            "cannot normalize Codex home {}: {error}",
            codex_home.display()
        )
    })?;
    Ok(Context {
        workspace,
        codex_home,
    })
}
fn config_home() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
}

pub fn load(path: &Path) -> Result<Config, String> {
    match fs::read_to_string(path) {
        Ok(text) => toml_edit::de::from_str(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}
pub fn save(path: &Path, config: &Config) -> Result<(), String> {
    let text = toml_edit::ser::to_string_pretty(config).map_err(|e| e.to_string())?;
    atomic_write(path, text.as_bytes(), 0o600)
}
pub fn atomic_write(path: &Path, contents: &[u8], mode: u32) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("invalid path: {}", path.display()))?;
    let parent_existed = parent.exists();
    fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    #[cfg(unix)]
    if !parent_existed {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    refuse_symlink(path)?;
    let temp = parent.join(format!(
        ".{}.skillwick-{}",
        path.file_name().and_then(|v| v.to_str()).unwrap_or("file"),
        std::process::id()
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode);
    }
    let mut file = options
        .open(&temp)
        .map_err(|e| format!("{}: {e}", temp.display()))?;
    let result = (|| {
        file.write_all(contents).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
pub fn refuse_symlink(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        Err(format!(
            "refusing symlinked destination: {}",
            path.display()
        ))
    } else {
        Ok(())
    }
}
