use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

pub const CONFIG_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Discovery {
    #[default]
    Auto,
    Explicit,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub path: PathBuf,
    #[serde(default)]
    pub roots: Vec<PathBuf>,
    #[serde(default)]
    pub discovery: Discovery,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Codex,
    Claude,
    #[default]
    None,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub discovery: Discovery,
    #[serde(default)]
    pub roots: Vec<PathBuf>,
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub agents: Vec<Agent>,
    #[serde(default)]
    pub instructions_file: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            discovery: Discovery::Auto,
            roots: Vec::new(),
            projects: Vec::new(),
            agents: Vec::new(),
            instructions_file: None,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, String> {
        load(path)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        save(path, self)
    }

    pub fn default_path() -> PathBuf {
        default_path()
    }

    pub fn cache_path() -> PathBuf {
        cache_path()
    }

    pub fn state_dir() -> PathBuf {
        state_dir()
    }
}

pub fn default_path() -> PathBuf {
    config_home().join("skillwick/config.toml")
}

pub fn cache_path() -> PathBuf {
    env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".cache"))
        .join("skillwick/index-v4.sqlite")
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

pub fn codex_home() -> PathBuf {
    env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".codex"))
}

pub fn claude_home() -> PathBuf {
    env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".claude"))
}

pub fn normalize_cwd(cwd: &Path) -> Result<PathBuf, String> {
    fs::canonicalize(cwd)
        .map_err(|error| format!("cannot normalize workspace {}: {error}", cwd.display()))
}

fn config_home() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
}

pub fn load(path: &Path) -> Result<Config, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };

    let document = text.parse::<toml_edit::DocumentMut>().map_err(|error| {
        format!(
            "invalid Skillwick configuration at {}: {error}; back it up and re-run `skillwick init`",
            path.display()
        )
    })?;
    if let Some(version) = document
        .get("version")
        .and_then(toml_edit::Item::as_integer)
    {
        if version != i64::from(CONFIG_VERSION) {
            return Err(format!(
                "unsupported Skillwick configuration version {version} at {}; back it up and re-run `skillwick init`",
                path.display()
            ));
        }
    }
    if ["agent", "inventory", "hooks", "hook", "router"]
        .iter()
        .any(|key| document.get(key).is_some())
    {
        return Err(format!(
            "obsolete Skillwick configuration at {}; back it up and re-run `skillwick init`",
            path.display()
        ));
    }

    let config = toml_edit::de::from_str::<Config>(&text).map_err(|error| {
        let detail = error.to_string();
        if detail.contains("unknown field") || detail.contains("missing field") {
            format!(
                "invalid Skillwick configuration at {}: {detail}; back it up and re-run `skillwick init`",
                path.display()
            )
        } else {
            format!("{}: {detail}", path.display())
        }
    })?;
    if config.version != CONFIG_VERSION {
        return Err(format!(
            "unsupported Skillwick configuration version {} at {}; back it up and re-run `skillwick init`",
            config.version,
            path.display()
        ));
    }
    Ok(config)
}

pub fn save(path: &Path, config: &Config) -> Result<(), String> {
    if config.version != CONFIG_VERSION {
        return Err(format!(
            "unsupported Skillwick configuration version {}; expected {}",
            config.version, CONFIG_VERSION
        ));
    }
    let text = toml_edit::ser::to_string_pretty(config).map_err(|e| e.to_string())?;
    atomic_write(path, text.as_bytes(), 0o600)
}

pub fn atomic_write(path: &Path, contents: &[u8], mode: u32) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("invalid path: {}", path.display()))?;
    let parent = if parent.as_os_str().is_empty() {
        Path::new(".")
    } else {
        parent
    };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_versioned_auto_discovery() {
        let config = Config::default();
        assert_eq!(config.version, CONFIG_VERSION);
        assert_eq!(config.discovery, Discovery::Auto);
        assert!(config.roots.is_empty());
        assert!(config.projects.is_empty());
        assert!(config.agents.is_empty());
    }

    #[test]
    fn config_round_trip_rejects_unknown_fields() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.toml");
        let config = Config {
            version: CONFIG_VERSION,
            discovery: Discovery::Explicit,
            roots: vec![temp.path().join("skills")],
            projects: vec![Project {
                path: temp.path().to_path_buf(),
                roots: Vec::new(),
                discovery: Discovery::Auto,
            }],
            agents: vec![Agent::Codex, Agent::Claude],
            instructions_file: Some(temp.path().join("AGENTS.md")),
        };
        save(&path, &config).unwrap();
        assert_eq!(load(&path).unwrap(), config);
        fs::write(&path, "version = 1\nunknown = true\n").unwrap();
        let error = load(&path).unwrap_err();
        assert!(error.contains("unknown field"));
        assert!(error.contains("re-run `skillwick init`"));
    }

    #[test]
    fn old_configuration_is_actionable_and_not_accepted() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.toml");
        fs::write(&path, "roots = []\nagent = \"none\"\n").unwrap();
        let error = load(&path).unwrap_err();
        assert!(error.contains("obsolete Skillwick configuration"));
        assert!(error.contains("back it up"));
        assert!(error.contains("re-run `skillwick init`"));
    }
}
