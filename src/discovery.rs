use crate::config::{self, Config, Discovery};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub const CODEX_PLUGIN_TIMEOUT: Duration = Duration::from_secs(10);
pub const MAX_PLUGIN_OUTPUT: usize = 8 * 1024 * 1024;
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SourceSpec {
    pub provider: String,
    pub scope: String,
    pub root: String,
    pub plugin_id: Option<String>,
    pub version: Option<String>,
    pub provenance: String,
}

#[derive(Debug, Default)]
pub struct Report {
    pub sources: Vec<SourceSpec>,
    pub configured_sources: Vec<SourceSpec>,
    pub configured_roots: Vec<(String, String)>,
    pub diagnostics: Vec<String>,
    pub complete: bool,
}

impl Report {
    pub fn roots(&self) -> Vec<(PathBuf, String)> {
        self.sources
            .iter()
            .map(|source| (PathBuf::from(&source.root), source.scope.clone()))
            .collect()
    }
}

struct Builder {
    cwd: PathBuf,
    current: Vec<SourceSpec>,
    configured: Vec<SourceSpec>,
    configured_roots: Vec<(String, String)>,
    diagnostics: Vec<String>,
    complete: bool,
}

impl Builder {
    fn new(cwd: &Path) -> Self {
        Self {
            cwd: cwd.to_path_buf(),
            current: Vec::new(),
            configured: Vec::new(),
            configured_roots: Vec::new(),
            diagnostics: Vec::new(),
            complete: true,
        }
    }

    fn add_global(
        &mut self,
        provider: &str,
        path: &Path,
        provenance: String,
        plugin_id: Option<String>,
        version: Option<String>,
        required: bool,
    ) {
        self.add(
            provider, "global", path, provenance, plugin_id, version, None, required, None, true,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn add_global_current(
        &mut self,
        provider: &str,
        path: &Path,
        provenance: String,
        plugin_id: Option<String>,
        version: Option<String>,
        required: bool,
        current: bool,
    ) {
        self.add(
            provider, "global", path, provenance, plugin_id, version, None, required, None, current,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn add_project(
        &mut self,
        project: &Path,
        provider: &str,
        path: &Path,
        provenance: String,
        plugin_id: Option<String>,
        version: Option<String>,
        required: bool,
    ) {
        self.add(
            provider,
            "project",
            path,
            provenance,
            plugin_id,
            version,
            Some(project),
            required,
            Some(project),
            true,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        provider: &str,
        scope: &str,
        path: &Path,
        provenance: String,
        plugin_id: Option<String>,
        version: Option<String>,
        configured_project: Option<&Path>,
        required: bool,
        applicable_project: Option<&Path>,
        include_current: bool,
    ) {
        let Some(root) = self.canonical_root(path, required) else {
            return;
        };
        let Some(root_text) = root.to_str() else {
            self.invalid_path(path, "source root is not UTF-8");
            return;
        };
        let spec = SourceSpec {
            provider: provider.to_owned(),
            scope: scope.to_owned(),
            root: root_text.to_owned(),
            plugin_id,
            version,
            provenance,
        };
        let key = SourceKey::from(&spec);
        if !self
            .configured
            .iter()
            .any(|source| SourceKey::from(source) == key)
        {
            self.configured.push(spec.clone());
        }
        let scope_key = configured_project
            .and_then(Path::to_str)
            .map(|project| format!("project:{project}"))
            .unwrap_or_else(|| "shared".to_owned());
        if !self
            .configured_roots
            .iter()
            .any(|(existing_scope, existing_root)| {
                existing_scope == &scope_key && existing_root == root_text
            })
        {
            self.configured_roots
                .push((scope_key, root_text.to_owned()));
        }
        if include_current
            && applicable_project.is_none_or(|project| self.cwd.starts_with(project))
            && !self
                .current
                .iter()
                .any(|source| SourceKey::from(source) == key)
        {
            self.current.push(spec);
        }
    }

    fn canonical_root(&mut self, path: &Path, required: bool) -> Option<PathBuf> {
        match fs::symlink_metadata(path) {
            Ok(metadata) => {
                if !metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
                    self.invalid_path(path, "source root is not a directory");
                    return None;
                }
                match fs::canonicalize(path) {
                    Ok(canonical) if canonical.is_dir() => Some(canonical),
                    Ok(_) => {
                        self.invalid_path(path, "source root is not a directory");
                        None
                    }
                    Err(error) => {
                        self.invalid_path(path, &error.to_string());
                        None
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound && !required => None,
            Err(error) => {
                if error.kind() == io::ErrorKind::NotFound {
                    if path.to_str().is_none() {
                        self.invalid_path(path, "source root is not UTF-8");
                    }
                } else {
                    self.invalid_path(path, &error.to_string());
                }
                if required {
                    Some(path.to_path_buf())
                } else {
                    None
                }
            }
        }
    }

    fn invalid_path(&mut self, path: &Path, detail: &str) {
        let path = path.to_string_lossy();
        self.diagnostics.push(format!("{path}: {detail}"));
        self.complete = false;
    }

    fn finish(mut self) -> Report {
        self.current.sort_by(source_order);
        self.configured.sort_by(source_order);
        self.configured_roots.sort();
        Report {
            sources: self.current,
            configured_sources: self.configured,
            configured_roots: self.configured_roots,
            diagnostics: self.diagnostics,
            complete: self.complete,
        }
    }
}

#[derive(PartialEq, Eq)]
struct SourceKey<'a> {
    provider: &'a str,
    scope: &'a str,
    root: &'a str,
    plugin_id: Option<&'a str>,
    version: Option<&'a str>,
    provenance: &'a str,
}

impl<'a> From<&'a SourceSpec> for SourceKey<'a> {
    fn from(source: &'a SourceSpec) -> Self {
        Self {
            provider: &source.provider,
            scope: &source.scope,
            root: &source.root,
            plugin_id: source.plugin_id.as_deref(),
            version: source.version.as_deref(),
            provenance: &source.provenance,
        }
    }
}

fn source_order(left: &SourceSpec, right: &SourceSpec) -> std::cmp::Ordering {
    (
        &left.provider,
        &left.scope,
        &left.root,
        &left.plugin_id,
        &left.version,
        &left.provenance,
    )
        .cmp(&(
            &right.provider,
            &right.scope,
            &right.root,
            &right.plugin_id,
            &right.version,
            &right.provenance,
        ))
}

pub fn discover(settings: &Config, cwd: &Path) -> Report {
    let normalized_cwd = match fs::canonicalize(cwd) {
        Ok(path) => path,
        Err(_) => cwd.to_path_buf(),
    };
    let mut builder = Builder::new(&normalized_cwd);

    for root in &settings.roots {
        builder.add_global(
            "custom",
            root,
            format!("custom root: {}", root.to_string_lossy()),
            None,
            None,
            true,
        );
    }

    if settings.discovery == Discovery::Auto {
        let home = config::home();
        builder.add_global(
            "agents",
            &home.join(".agents/skills"),
            format!(
                "global convention: {}/.agents/skills",
                home.to_string_lossy()
            ),
            None,
            None,
            false,
        );
        let codex_home = config::codex_home();
        builder.add_global(
            "codex",
            &codex_home.join("skills"),
            format!(
                "Codex skills: {}",
                codex_home.join("skills").to_string_lossy()
            ),
            None,
            None,
            false,
        );
        let claude_home = config::claude_home();
        builder.add_global(
            "claude",
            &claude_home.join("skills"),
            format!(
                "Claude skills: {}",
                claude_home.join("skills").to_string_lossy()
            ),
            None,
            None,
            false,
        );

        if codex_plugin_indicator(&codex_home) {
            match codex_plugin_roots(&codex_home) {
                Ok(roots) => {
                    for root in roots {
                        builder.add_global(
                            "codex-plugin",
                            &root.root,
                            root.provenance,
                            Some(root.plugin_id),
                            Some(root.version),
                            true,
                        );
                    }
                }
                Err(error) => {
                    builder.diagnostics.push(error);
                    builder.complete = false;
                }
            }
        }

        if claude_plugin_indicator(&claude_home, settings, &builder.cwd) {
            if let Err(error) = add_claude_plugins(&mut builder, &claude_home, settings) {
                builder.diagnostics.push(error);
                builder.complete = false;
            }
        }
    }

    for project in &settings.projects {
        let Ok(path) = fs::canonicalize(&project.path) else {
            continue;
        };
        for root in &project.roots {
            builder.add_project(
                &path,
                "custom",
                root,
                format!("project root: {}", root.to_string_lossy()),
                None,
                None,
                true,
            );
        }
        if project.discovery == Discovery::Auto {
            for (provider, relative) in [
                ("agents", ".agents/skills"),
                ("codex", ".codex/skills"),
                ("claude", ".claude/skills"),
            ] {
                let root = path.join(relative);
                builder.add_project(
                    &path,
                    provider,
                    &root,
                    format!("project convention: {}", root.to_string_lossy()),
                    None,
                    None,
                    false,
                );
            }
        }
    }

    builder.finish()
}

pub fn explicit_sources(
    cwd: &Path,
    shared: &[PathBuf],
    projects: &[crate::config::Project],
) -> Report {
    let mut builder = Builder::new(cwd);
    for root in shared {
        builder.add_global(
            "custom",
            root,
            format!("custom root: {}", root.to_string_lossy()),
            None,
            None,
            true,
        );
    }
    for project in projects {
        let Ok(path) = fs::canonicalize(&project.path) else {
            continue;
        };
        for root in &project.roots {
            builder.add_project(
                &path,
                "custom",
                root,
                format!("project root: {}", root.to_string_lossy()),
                None,
                None,
                true,
            );
        }
    }
    builder.finish()
}

fn codex_plugin_indicator(home: &Path) -> bool {
    [
        home.join("plugins/cache"),
        home.join("plugins/installed_plugins.json"),
        home.join("plugins/.plugin-appserver"),
    ]
    .iter()
    .any(|path| fs::symlink_metadata(path).is_ok())
}

struct PluginRoot {
    root: PathBuf,
    plugin_id: String,
    version: String,
    provenance: String,
}

fn codex_plugin_roots(home: &Path) -> Result<Vec<PluginRoot>, String> {
    let output = run_codex_plugin_list()?;
    let document: Value = serde_json::from_slice(&output)
        .map_err(|error| format!("invalid Codex plugin list JSON: {error}"))?;
    let installed = document
        .get("installed")
        .and_then(Value::as_array)
        .ok_or("unsupported Codex plugin list schema: installed must be an array")?;
    let cache = home.join("plugins/cache");
    let cache_canonical = fs::canonicalize(&cache).ok();
    let mut discovered = Vec::new();
    let mut seen = HashSet::new();
    let mut active_versions = BTreeMap::new();
    for item in installed {
        let object = item
            .as_object()
            .ok_or("unsupported Codex plugin list schema: installed entry must be an object")?;
        let installed = object
            .get("installed")
            .and_then(Value::as_bool)
            .ok_or("unsupported Codex plugin list schema: installed entry missing installed")?;
        if !installed {
            continue;
        }
        let enabled = object
            .get("enabled")
            .and_then(Value::as_bool)
            .ok_or("unsupported Codex plugin list schema: installed entry missing enabled")?;
        if !enabled {
            continue;
        }
        let plugin_id = match object.get("pluginId").and_then(Value::as_str) {
            Some(value) => value.to_owned(),
            None => {
                let name = object
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or("unsupported Codex plugin list schema: missing plugin name")?;
                let marketplace = object
                    .get("marketplaceName")
                    .and_then(Value::as_str)
                    .ok_or("unsupported Codex plugin list schema: missing marketplace name")?;
                format!("{name}@{marketplace}")
            }
        };
        let (name, marketplace) = plugin_id
            .split_once('@')
            .ok_or_else(|| format!("invalid Codex plugin id: {plugin_id}"))?;
        valid_component(name, "Codex plugin name")?;
        valid_component(marketplace, "Codex marketplace name")?;
        let version = object
            .get("version")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or("unsupported Codex plugin list schema: missing plugin version")?
            .to_owned();
        valid_component(&version, "Codex plugin version")?;
        if let Some(previous) = active_versions.insert(plugin_id.clone(), version.clone()) {
            if previous != version {
                return Err(format!(
                    "multiple active Codex plugin versions for {plugin_id}: {previous}, {version}"
                ));
            }
        }
        let key = format!("{plugin_id}@{version}");
        if !seen.insert(key) {
            continue;
        }
        let cache_package = cache.join(marketplace).join(name).join(&version);
        let source_package = object
            .get("source")
            .and_then(Value::as_object)
            .and_then(|source| source.get("path"))
            .and_then(Value::as_str)
            .map(PathBuf::from);
        let package = if fs::symlink_metadata(&cache_package).is_ok() {
            cache_package.clone()
        } else if let Some(source_package) = source_package {
            if !source_package.is_absolute() {
                return Err(format!(
                    "Codex active plugin package path is not absolute: {}",
                    source_package.display()
                ));
            }
            if fs::symlink_metadata(&source_package).is_ok() {
                source_package
            } else if explicitly_remote_only(object) {
                continue;
            } else {
                return Err(format!(
                    "Codex active plugin package is missing: {}",
                    source_package.display()
                ));
            }
        } else if explicitly_remote_only(object) {
            continue;
        } else {
            return Err(format!(
                "Codex active plugin package is missing: {}",
                cache_package.display()
            ));
        };
        let package_canonical = fs::canonicalize(&package).map_err(|error| {
            format!(
                "Codex active plugin package cannot be read: {}: {error}",
                package.display()
            )
        })?;
        if package == cache_package {
            if let Some(cache_canonical) = &cache_canonical {
                if !package_canonical.starts_with(cache_canonical) {
                    return Err(format!(
                        "Codex plugin package escapes cache: {}",
                        package.display()
                    ));
                }
            }
        }
        let manifest_roots = manifest_roots(
            &package_canonical,
            ".codex-plugin/plugin.json",
            &plugin_id,
            &version,
        )?;
        for root in manifest_roots {
            roots_push(
                &mut discovered,
                PluginRoot {
                    root,
                    plugin_id: plugin_id.clone(),
                    version: version.clone(),
                    provenance: format!("Codex plugin {plugin_id}@{version}"),
                },
            );
        }
    }
    Ok(discovered)
}

fn roots_push(roots: &mut Vec<PluginRoot>, candidate: PluginRoot) {
    if !roots.iter().any(|existing| {
        existing.root == candidate.root
            && existing.plugin_id == candidate.plugin_id
            && existing.version == candidate.version
    }) {
        roots.push(candidate);
    }
}

fn run_codex_plugin_list() -> Result<Vec<u8>, String> {
    run_bounded_command("codex", CODEX_PLUGIN_TIMEOUT)
}

fn run_bounded_command(program: &str, timeout: Duration) -> Result<Vec<u8>, String> {
    let mut child = Command::new(program)
        .args(["plugin", "list", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot run `{program} plugin list --json`: {error}"))?;
    let overflow = Arc::new(AtomicBool::new(false));
    let Some(stdout_pipe) = child.stdout.take() else {
        terminate_child(&mut child);
        return Err("Codex plugin command did not provide stdout".into());
    };
    let Some(stderr_pipe) = child.stderr.take() else {
        terminate_child(&mut child);
        return Err("Codex plugin command did not provide stderr".into());
    };
    let stdout = spawn_reader(stdout_pipe, Arc::clone(&overflow));
    let stderr = spawn_reader(stderr_pipe, Arc::clone(&overflow));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if overflow.load(Ordering::Relaxed) => {
                terminate_child(&mut child);
                return Err(format!(
                    "Codex plugin list output exceeds {MAX_PLUGIN_OUTPUT} bytes"
                ));
            }
            Ok(None) if Instant::now() >= deadline => {
                terminate_child(&mut child);
                return Err(format!(
                    "Codex plugin list timed out after {} seconds",
                    timeout.as_secs()
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                terminate_child(&mut child);
                return Err(format!("Codex plugin list wait failed: {error}"));
            }
        }
    };
    let stdout = receive_reader(stdout, deadline, "stdout")?;
    let stderr = receive_reader(stderr, deadline, "stderr")?;
    if overflow.load(Ordering::Relaxed) {
        return Err(format!(
            "Codex plugin list output exceeds {MAX_PLUGIN_OUTPUT} bytes"
        ));
    }
    if !status.success() {
        return Err(format!(
            "Codex plugin list failed ({}): {}",
            status,
            bounded_error_text(&stderr)
        ));
    }
    Ok(stdout)
}

fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    overflow: Arc<AtomicBool>,
) -> Receiver<Result<Vec<u8>, String>> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = (|| {
            let mut output = Vec::new();
            let mut buffer = [0u8; 16 * 1024];
            loop {
                let read = reader
                    .read(&mut buffer)
                    .map_err(|error| error.to_string())?;
                if read == 0 {
                    break;
                }
                if output.len().saturating_add(read) > MAX_PLUGIN_OUTPUT {
                    overflow.store(true, Ordering::Relaxed);
                    let remaining = MAX_PLUGIN_OUTPUT.saturating_sub(output.len());
                    output.extend_from_slice(&buffer[..remaining]);
                } else {
                    output.extend_from_slice(&buffer[..read]);
                }
            }
            Ok(output)
        })();
        let _ = sender.send(result);
    });
    receiver
}

fn receive_reader(
    receiver: Receiver<Result<Vec<u8>, String>>,
    deadline: Instant,
    stream: &str,
) -> Result<Vec<u8>, String> {
    receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => {
                format!("Codex plugin list timed out while reading {stream}")
            }
            mpsc::RecvTimeoutError::Disconnected => {
                format!("Codex plugin {stream} reader disconnected")
            }
        })?
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn bounded_error_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.trim().to_owned(),
        Err(_) => "Codex plugin command wrote non-UTF-8 diagnostics".to_owned(),
    }
}

fn valid_component(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains(['/', '\\'])
        || value.chars().any(char::is_control)
    {
        Err(format!("invalid {label}: {value:?}"))
    } else {
        Ok(())
    }
}

fn explicitly_remote_only(object: &serde_json::Map<String, Value>) -> bool {
    if object
        .get("remoteOnly")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || object
            .get("filesystem")
            .and_then(Value::as_bool)
            .is_some_and(|filesystem| !filesystem)
    {
        return true;
    }
    let Some(source) = object.get("source").and_then(Value::as_object) else {
        return false;
    };
    if source
        .get("remoteOnly")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || source
            .get("filesystem")
            .and_then(Value::as_bool)
            .is_some_and(|filesystem| !filesystem)
    {
        return true;
    }
    let source_type = source
        .get("sourceType")
        .or_else(|| source.get("type"))
        .or_else(|| source.get("source"))
        .and_then(Value::as_str);
    matches!(source_type, Some("remote-only"))
        || (matches!(source_type, Some("npm" | "remote" | "registry"))
            && source.get("path").is_none()
            && source.get("package").and_then(Value::as_str).is_some())
}

fn manifest_roots(
    package: &Path,
    manifest_relative: &str,
    plugin_id: &str,
    version: &str,
) -> Result<Vec<PathBuf>, String> {
    let manifest = package.join(manifest_relative);
    let manifest_canonical = fs::canonicalize(&manifest).map_err(|error| {
        format!(
            "active plugin {plugin_id}@{version} manifest cannot be read: {}: {error}",
            manifest.display()
        )
    })?;
    if !manifest_canonical.starts_with(package) {
        return Err(format!(
            "active plugin manifest escapes package: {}",
            manifest.display()
        ));
    }
    let bytes = read_bounded(&manifest, MAX_MANIFEST_BYTES)
        .map_err(|error| format!("{}: {error}", manifest.display()))?;
    let document: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid plugin manifest {}: {error}", manifest.display()))?;
    let object = document
        .as_object()
        .ok_or_else(|| format!("plugin manifest is not an object: {}", manifest.display()))?;
    if let Some(name) = object.get("name") {
        if name.as_str() != plugin_id.split_once('@').map(|(name, _)| name) {
            return Err(format!(
                "plugin manifest name mismatch: {}",
                manifest.display()
            ));
        }
    }
    if let Some(manifest_version) = object.get("version") {
        if manifest_version.as_str() != Some(version) {
            return Err(format!(
                "plugin manifest version mismatch: {}",
                manifest.display()
            ));
        }
    }
    let declared = match object.get("skills") {
        None => Vec::new(),
        Some(Value::String(path)) => vec![path.clone()],
        Some(Value::Array(paths)) => paths
            .iter()
            .map(|path| {
                path.as_str().map(str::to_owned).ok_or_else(|| {
                    format!(
                        "plugin manifest skills path is not a string: {}",
                        manifest.display()
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => {
            return Err(format!(
                "plugin manifest skills must be a string or array: {}",
                manifest.display()
            ))
        }
    };
    let mut paths = declared;
    if paths.is_empty() {
        paths.push("skills".to_owned());
    }
    let mut roots = Vec::new();
    for declared in paths {
        let path = package.join(&declared);
        if !fs::symlink_metadata(&path).is_ok() {
            if object.get("skills").is_some() {
                return Err(format!(
                    "active plugin skill path is missing: {}",
                    path.display()
                ));
            }
            continue;
        }
        let canonical = fs::canonicalize(&path).map_err(|error| {
            format!(
                "active plugin skill path cannot be read: {}: {error}",
                path.display()
            )
        })?;
        if !canonical.starts_with(package) || !canonical.is_dir() {
            return Err(format!(
                "active plugin skill path escapes package: {}",
                path.display()
            ));
        }
        if !roots.contains(&canonical) {
            roots.push(canonical);
        }
    }
    Ok(roots)
}

fn claude_plugin_indicator(home: &Path, settings: &Config, cwd: &Path) -> bool {
    if fs::symlink_metadata(home.join("plugins/installed_plugins.json")).is_ok() {
        return true;
    }
    let mut paths = vec![home.join("settings.json")];
    for project in &settings.projects {
        let Ok(path) = fs::canonicalize(&project.path) else {
            continue;
        };
        if cwd.starts_with(&path) {
            paths.push(path.join(".claude/settings.json"));
            paths.push(path.join(".claude/settings.local.json"));
        }
    }
    paths.iter().any(|path| fs::symlink_metadata(path).is_ok())
}

#[derive(Clone)]
struct ClaudeInstall {
    key: String,
    scope: String,
    install_path: PathBuf,
    version: String,
    project_path: Option<PathBuf>,
}

fn add_claude_plugins(builder: &mut Builder, home: &Path, settings: &Config) -> Result<(), String> {
    let installed_path = home.join("plugins/installed_plugins.json");
    let installs = if fs::symlink_metadata(&installed_path).is_ok() {
        parse_claude_installs(&installed_path)?
    } else {
        Vec::new()
    };
    let user_enabled = enabled_plugins(&home.join("settings.json"))?;
    let mut projects = Vec::new();
    for project in &settings.projects {
        let Ok(path) = fs::canonicalize(&project.path) else {
            continue;
        };
        let current = builder.cwd.starts_with(&path);
        let project_settings = match enabled_plugins(&path.join(".claude/settings.json")) {
            Ok(settings) => settings,
            Err(error) if current => return Err(error),
            Err(_) => continue,
        };
        let local_settings = match enabled_plugins(&path.join(".claude/settings.local.json")) {
            Ok(settings) => settings,
            Err(error) if current => return Err(error),
            Err(_) => continue,
        };
        let effective = merge_enabled(&user_enabled, &project_settings, &local_settings);
        projects.push((path, effective, current));
    }

    for (key, enabled) in &user_enabled {
        if *enabled {
            let candidates = applicable_installs(&installs, key, None);
            let candidate = choose_install(key, candidates)?
                .ok_or_else(|| format!("Claude enabled plugin has no installed package: {key}"))?;
            // Keep the user package in configured sources for cache
            // associations, but let an applicable project or local package
            // own the current view when it has higher precedence.
            let current = current_plugin_enabled(&projects, &builder.cwd, key).unwrap_or(true)
                && !current_project_install_selected(&projects, &installs, &builder.cwd, key)?;
            add_claude_install(builder, candidate, None, current)?;
        }
    }
    for (project, effective, current) in &projects {
        for (key, enabled) in effective {
            if !*enabled {
                continue;
            }
            let candidates = applicable_installs(&installs, key, Some(project));
            let Some(candidate) = choose_install(key, candidates)? else {
                if *current {
                    return Err(format!(
                        "Claude enabled plugin has no installed package for project {}: {key}",
                        project.display()
                    ));
                }
                continue;
            };
            // A user-scoped package already enabled globally needs no second
            // project association unless project settings enable it locally.
            if candidate.scope == "user" && user_enabled.get(key).copied().unwrap_or(false) {
                continue;
            }
            let include_current = *current
                && current_project_for_plugin(&projects, &builder.cwd, key)
                    .is_some_and(|selected| selected == project);
            add_claude_install(builder, candidate, Some(project), include_current)?;
        }
    }
    Ok(())
}

fn current_project_for_plugin<'a>(
    projects: &'a [(PathBuf, BTreeMap<String, bool>, bool)],
    cwd: &Path,
    key: &str,
) -> Option<&'a PathBuf> {
    projects
        .iter()
        .filter(|(project, _, current)| *current && cwd.starts_with(project))
        .max_by_key(|(project, _, _)| project.components().count())
        .filter(|(_, effective, _)| effective.get(key) == Some(&true))
        .map(|(project, _, _)| project)
}

fn current_project_install_selected(
    projects: &[(PathBuf, BTreeMap<String, bool>, bool)],
    installs: &[ClaudeInstall],
    cwd: &Path,
    key: &str,
) -> Result<bool, String> {
    let Some(project) = current_project_for_plugin(projects, cwd, key) else {
        return Ok(false);
    };
    let candidate = choose_install(key, applicable_installs(installs, key, Some(project)))?;
    Ok(candidate.is_some_and(|candidate| candidate.scope != "user"))
}

fn current_plugin_enabled(
    projects: &[(PathBuf, BTreeMap<String, bool>, bool)],
    cwd: &Path,
    key: &str,
) -> Option<bool> {
    projects
        .iter()
        .filter(|(project, _, _)| cwd.starts_with(project))
        .max_by_key(|(project, _, _)| project.components().count())
        .and_then(|(_, settings, _)| settings.get(key).copied())
}

fn add_claude_install(
    builder: &mut Builder,
    install: &ClaudeInstall,
    project: Option<&Path>,
    current: bool,
) -> Result<(), String> {
    if !install.install_path.is_absolute() {
        return Err(format!(
            "Claude plugin install path is not absolute: {}",
            install.install_path.display()
        ));
    }
    let package = fs::canonicalize(&install.install_path).map_err(|error| {
        format!(
            "active Claude plugin path cannot be read: {}: {error}",
            install.install_path.display()
        )
    })?;
    if !package.is_dir() {
        return Err(format!(
            "active Claude plugin path is not a directory: {}",
            package.display()
        ));
    }
    let roots = manifest_roots(
        &package,
        ".claude-plugin/plugin.json",
        &install.key,
        &install.version,
    )?;
    for root in roots {
        let provenance = format!("Claude plugin {}@{}", install.key, install.version);
        if let Some(project) = project {
            builder.add(
                "claude-plugin",
                "project",
                &root,
                provenance,
                Some(install.key.clone()),
                Some(install.version.clone()),
                Some(project),
                true,
                Some(project),
                current,
            );
        } else {
            builder.add_global_current(
                "claude-plugin",
                &root,
                provenance,
                Some(install.key.clone()),
                Some(install.version.clone()),
                true,
                current,
            );
        }
    }
    Ok(())
}

fn parse_claude_installs(path: &Path) -> Result<Vec<ClaudeInstall>, String> {
    let document = read_json(path)?;
    let version = document
        .get("version")
        .and_then(Value::as_u64)
        .ok_or("unsupported Claude installed_plugins.json schema: missing version")?;
    if version != 2 {
        return Err(format!(
            "unsupported Claude installed_plugins.json version: {version}"
        ));
    }
    let plugins = document
        .get("plugins")
        .and_then(Value::as_object)
        .ok_or("unsupported Claude installed_plugins.json schema: plugins must be an object")?;
    let mut installs = Vec::new();
    for (key, entries) in plugins {
        let entries = entries
            .as_array()
            .ok_or_else(|| format!("Claude installed plugin entries must be an array: {key}"))?;
        for entry in entries {
            let object = entry
                .as_object()
                .ok_or_else(|| format!("Claude installed plugin entry must be an object: {key}"))?;
            let scope = object
                .get("scope")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("Claude installed plugin entry missing scope: {key}"))?;
            if !matches!(scope, "user" | "project" | "local") {
                return Err(format!("unsupported Claude plugin scope {scope:?}: {key}"));
            }
            let install_path = object
                .get("installPath")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!("Claude installed plugin entry missing installPath: {key}")
                })?;
            let version = object
                .get("version")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| format!("Claude installed plugin entry missing version: {key}"))?;
            let project_path = object
                .get("projectPath")
                .and_then(Value::as_str)
                .map(PathBuf::from);
            if matches!(scope, "project" | "local") && project_path.is_none() {
                return Err(format!(
                    "Claude project plugin entry missing projectPath: {key}"
                ));
            }
            installs.push(ClaudeInstall {
                key: key.clone(),
                scope: scope.to_owned(),
                install_path: PathBuf::from(install_path),
                version: version.to_owned(),
                project_path,
            });
        }
    }
    Ok(installs)
}

fn applicable_installs<'a>(
    installs: &'a [ClaudeInstall],
    key: &str,
    project: Option<&Path>,
) -> Vec<&'a ClaudeInstall> {
    installs
        .iter()
        .filter(|install| {
            if install.key != key {
                return false;
            }
            match (&install.scope[..], project) {
                ("user", _) => true,
                ("project" | "local", Some(project)) => install
                    .project_path
                    .as_deref()
                    .and_then(|path| fs::canonicalize(path).ok())
                    .is_some_and(|path| path == project),
                _ => false,
            }
        })
        .collect()
}

fn choose_install<'a>(
    key: &str,
    candidates: Vec<&'a ClaudeInstall>,
) -> Result<Option<&'a ClaudeInstall>, String> {
    if candidates.is_empty() {
        return Ok(None);
    }
    let highest = if candidates
        .iter()
        .any(|candidate| candidate.scope == "local")
    {
        "local"
    } else if candidates
        .iter()
        .any(|candidate| candidate.scope == "project")
    {
        "project"
    } else {
        "user"
    };
    let candidates = candidates
        .into_iter()
        .filter(|candidate| candidate.scope == highest)
        .collect::<Vec<_>>();
    if candidates.len() > 1 {
        return Err(format!("ambiguous active Claude plugin package: {key}"));
    }
    Ok(candidates.into_iter().next())
}

fn enabled_plugins(path: &Path) -> Result<BTreeMap<String, bool>, String> {
    if !fs::symlink_metadata(path).is_ok() {
        return Ok(BTreeMap::new());
    }
    let document = read_json(path)?;
    let Some(enabled) = document.get("enabledPlugins") else {
        return Ok(BTreeMap::new());
    };
    let enabled = enabled.as_object().ok_or_else(|| {
        format!(
            "Claude enabledPlugins must be an object: {}",
            path.display()
        )
    })?;
    let mut result = BTreeMap::new();
    for (key, value) in enabled {
        let value = value
            .as_bool()
            .ok_or_else(|| format!("Claude enabledPlugins value must be boolean: {key}"))?;
        result.insert(key.clone(), value);
    }
    Ok(result)
}

fn merge_enabled(
    user: &BTreeMap<String, bool>,
    project: &BTreeMap<String, bool>,
    local: &BTreeMap<String, bool>,
) -> BTreeMap<String, bool> {
    let mut result = user.clone();
    result.extend(project.iter().map(|(key, value)| (key.clone(), *value)));
    result.extend(local.iter().map(|(key, value)| (key.clone(), *value)));
    result
}

fn read_json(path: &Path) -> Result<Value, String> {
    let bytes = read_bounded(path, MAX_PLUGIN_OUTPUT)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid JSON {}: {error}", path.display()))
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    if file.metadata().map_err(|error| error.to_string())?.len() > limit as u64 {
        return Err(format!("file exceeds {limit} byte limit"));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > limit {
        return Err(format!("file exceeds {limit} byte limit"));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn explicit_sources_are_scoped_to_registered_projects() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let child = project.join("child");
        let sibling = temp.path().join("sibling");
        let root = project.join(".agents/skills");
        fs::create_dir_all(&child).unwrap();
        fs::create_dir_all(&sibling).unwrap();
        fs::create_dir_all(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let settings = Config {
            version: config::CONFIG_VERSION,
            discovery: Discovery::Auto,
            roots: Vec::new(),
            projects: vec![config::Project {
                path: project.clone(),
                roots: Vec::new(),
                discovery: Discovery::Auto,
            }],
            agents: Vec::new(),
            instructions_file: None,
        };
        let in_project = discover(&settings, &child);
        let outside = discover(&settings, &sibling);
        assert!(in_project
            .sources
            .iter()
            .any(|source| source.root == root.to_str().unwrap()));
        assert!(!outside
            .sources
            .iter()
            .any(|source| source.root == root.to_str().unwrap()));
    }

    #[test]
    fn bounded_command_kills_timeout_and_limits_output() {
        let temp = tempfile::tempdir().unwrap();
        let script = temp.path().join("codex");
        write(&script, "#!/bin/sh\nsleep 2\n");
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();
        let timeout = run_bounded_command(script.to_str().unwrap(), Duration::from_millis(20));
        assert!(timeout.unwrap_err().contains("timed out"));
    }

    #[test]
    fn bounded_command_deadline_includes_inherited_pipe_readers() {
        let temp = tempfile::tempdir().unwrap();
        let script = temp.path().join("codex");
        write(&script, "#!/bin/sh\n(sleep 2) &\nwait\n");
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();
        let started = Instant::now();
        let timeout = run_bounded_command(script.to_str().unwrap(), Duration::from_millis(20));
        assert!(timeout.unwrap_err().contains("timed out"));
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "reader exceeded command deadline: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn manifest_declared_skill_path_must_stay_inside_package() {
        let temp = tempfile::tempdir().unwrap();
        let package = temp.path().join("package");
        write(
            &package.join(".codex-plugin/plugin.json"),
            r#"{"name":"x","version":"1","skills":["../outside"]}"#,
        );
        fs::create_dir_all(temp.path().join("outside")).unwrap();
        let result = manifest_roots(&package, ".codex-plugin/plugin.json", "x@m", "1");
        assert!(result.is_err());
    }

    #[test]
    fn source_spec_serializes_optional_plugin_identity() {
        let source = SourceSpec {
            provider: "codex-plugin".into(),
            scope: "global".into(),
            root: "/tmp/skills".into(),
            plugin_id: Some("x@m".into()),
            version: Some("1".into()),
            provenance: "Codex plugin x@m@1".into(),
        };
        let json = serde_json::to_value(source).unwrap();
        assert_eq!(json["provider"], "codex-plugin");
        assert_eq!(json["plugin_id"], "x@m");
    }

    fn write_claude_plugin(path: &Path, name: &str, version: &str) {
        write(
            &path.join(".claude-plugin/plugin.json"),
            &format!(r#"{{"name":"{name}","version":"{version}","skills":"skills"}}"#),
        );
        fs::create_dir_all(path.join("skills")).unwrap();
    }

    #[test]
    fn claude_project_and_local_installs_override_user_current_source() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("claude");
        let project = temp.path().join("project");
        let user_package = temp.path().join("user-package");
        let project_package = temp.path().join("project-package");
        let local_package = temp.path().join("local-package");
        fs::create_dir_all(&project).unwrap();
        write_claude_plugin(&user_package, "foo", "1");
        write_claude_plugin(&project_package, "foo", "2");
        write_claude_plugin(&local_package, "foo", "3");
        write(
            &home.join("plugins/installed_plugins.json"),
            &format!(
                r#"{{
                    "version": 2,
                    "plugins": {{
                        "foo@market": [
                            {{"scope":"user","installPath":"{}","version":"1"}},
                            {{"scope":"project","installPath":"{}","version":"2","projectPath":"{}"}},
                            {{"scope":"local","installPath":"{}","version":"3","projectPath":"{}"}}
                        ]
                    }}
                }}"#,
                user_package.display(),
                project_package.display(),
                project.display(),
                local_package.display(),
                project.display(),
            ),
        );
        write(
            &home.join("settings.json"),
            r#"{"enabledPlugins":{"foo@market":true}}"#,
        );
        write(
            &project.join(".claude/settings.json"),
            r#"{"enabledPlugins":{"foo@market":true}}"#,
        );
        write(
            &project.join(".claude/settings.local.json"),
            r#"{"enabledPlugins":{"foo@market":true}}"#,
        );

        let settings = Config {
            version: config::CONFIG_VERSION,
            discovery: Discovery::Auto,
            roots: Vec::new(),
            projects: vec![config::Project {
                path: project.clone(),
                roots: Vec::new(),
                discovery: Discovery::Auto,
            }],
            agents: Vec::new(),
            instructions_file: None,
        };
        let project = fs::canonicalize(project).unwrap();
        let mut builder = Builder::new(&project);
        add_claude_plugins(&mut builder, &home, &settings).unwrap();
        let report = builder.finish();

        assert_eq!(report.sources.len(), 1);
        assert_eq!(report.sources[0].scope, "project");
        assert_eq!(report.sources[0].version.as_deref(), Some("3"));
        assert!(report
            .configured_sources
            .iter()
            .any(|source| source.scope == "global" && source.version.as_deref() == Some("1")));
        assert!(report
            .configured_sources
            .iter()
            .any(|source| source.scope == "project" && source.version.as_deref() == Some("3")));
        assert!(!report
            .sources
            .iter()
            .any(|source| source.version.as_deref() == Some("1")));
    }
}
