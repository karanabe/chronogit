//! Trusted user-level language-server profiles and deterministic file routing.

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::domain::{RepoPath, RepositoryRoot};
use crate::lsp::LspError;

const MAX_PROFILES: usize = 32;
const MAX_COMMAND_PARTS: usize = 64;
const MAX_VALUE_BYTES: usize = 4096;
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_EXTENSIONS: usize = 32;
const MAX_ROOT_MARKERS: usize = 64;

/// A path-qualified language-server configuration failure.
#[derive(Debug)]
pub struct LspConfigError {
    path: Option<PathBuf>,
    detail: String,
}

impl Display for LspConfigError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        if let Some(path) = &self.path {
            write!(
                formatter,
                "could not load LSP config {}: {}",
                path.display(),
                self.detail
            )
        } else {
            formatter.write_str(&self.detail)
        }
    }
}

impl Error for LspConfigError {}

/// A validated, shell-free command and routing profile for one language server.
#[derive(Clone, Debug)]
pub struct ServerProfile {
    id: String,
    language_id: String,
    extensions: Vec<String>,
    command: Vec<String>,
    root_markers: Vec<String>,
    initialization_options: Value,
    workspace_data: bool,
    install_hint: String,
}

impl ServerProfile {
    /// Returns the stable profile identifier used by `--lsp`.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the LSP `languageId` sent with opened documents.
    #[must_use]
    pub fn language_id(&self) -> &str {
        &self.language_id
    }

    /// Returns the normalized extensions claimed by this profile.
    #[must_use]
    pub fn extensions(&self) -> &[String] {
        &self.extensions
    }

    pub(crate) fn initialization_options(&self) -> &Value {
        &self.initialization_options
    }

    pub(crate) fn install_hint(&self) -> &str {
        &self.install_hint
    }

    pub(crate) fn needs_workspace_data(&self) -> bool {
        self.workspace_data
    }

    pub(crate) fn workspace_root(&self, repository: &RepositoryRoot, path: &RepoPath) -> PathBuf {
        let repository_path = repository.as_path();
        let relative = PathBuf::from(path.to_os_string());
        let mut directory = repository_path
            .join(relative)
            .parent()
            .map(Path::to_path_buf);
        while let Some(candidate) = directory {
            if self
                .root_markers
                .iter()
                .any(|marker| candidate.join(marker).exists())
            {
                return candidate;
            }
            if candidate == repository_path {
                break;
            }
            directory = candidate.parent().map(Path::to_path_buf);
        }
        repository_path.to_path_buf()
    }

    pub(crate) fn command(
        &self,
        workspace_root: &Path,
        workspace_data: Option<&Path>,
        cache_dir: &Path,
    ) -> Result<(OsString, Vec<OsString>), LspError> {
        let expand = |part: &str| -> Result<OsString, LspError> {
            match part {
                "{workspace_root}" => Ok(workspace_root.as_os_str().to_owned()),
                "{workspace_data}" => workspace_data
                    .map(|path| path.as_os_str().to_owned())
                    .ok_or_else(|| LspError::Process("profile requires workspace data".to_owned())),
                "{workspace_config}" => workspace_data
                    .map(|path| path.join("configuration").into_os_string())
                    .ok_or_else(|| LspError::Process("profile requires workspace data".to_owned())),
                "{cache_dir}" => Ok(cache_dir.as_os_str().to_owned()),
                _ => Ok(OsString::from(part)),
            }
        };
        let executable = expand(&self.command[0])?;
        let arguments = self.command[1..]
            .iter()
            .map(|part| expand(part))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((executable, arguments))
    }
}

/// The explicitly enabled, validated set of language-server profiles.
#[derive(Clone, Debug, Default)]
pub struct LspConfig {
    profiles: Vec<ServerProfile>,
}

impl LspConfig {
    /// Loads built-in profiles plus optional trusted user-level overrides.
    ///
    /// `enabled` is normally populated by repeatable `--lsp PROFILE` options.
    /// An empty list disables LSP completely. An explicit config path must
    /// exist; an implicit XDG config is optional.
    ///
    /// # Errors
    ///
    /// Returns an error for unreadable TOML, unknown requested profiles, unsafe
    /// commands, or values exceeding configured bounds.
    pub fn load(enabled: &[String], explicit_path: Option<&Path>) -> Result<Self, LspConfigError> {
        if enabled.is_empty() {
            return Ok(Self::disabled());
        }
        let mut profiles = builtin_profiles()
            .into_iter()
            .map(|profile| (profile.id.clone(), profile))
            .collect::<HashMap<_, _>>();
        if profiles.len() > MAX_PROFILES {
            return Err(config_error(None, "too many built-in LSP profiles"));
        }

        let explicit = explicit_path.is_some();
        let config_path = explicit_path.map(Path::to_path_buf).or_else(default_path);
        if let Some(path) = config_path {
            match fs::metadata(&path) {
                Ok(metadata) if metadata.len() > MAX_CONFIG_BYTES => {
                    return Err(config_error(
                        Some(&path),
                        "LSP config exceeded the 1 MiB safety limit",
                    ));
                }
                Ok(_) => {}
                Err(error) if !explicit && error.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(Self {
                        profiles: selected_profiles(enabled, &profiles, explicit_path)?,
                    });
                }
                Err(error) => return Err(config_error(Some(&path), error.to_string())),
            }
            match fs::read_to_string(&path) {
                Ok(source) => merge_user_profiles(&path, &source, &mut profiles)?,
                Err(error) => return Err(config_error(Some(&path), error.to_string())),
            }
        }
        Ok(Self {
            profiles: selected_profiles(enabled, &profiles, explicit_path)?,
        })
    }

    /// Creates a configuration with no enabled servers.
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            profiles: Vec::new(),
        }
    }

    /// Reports whether no language server can be started.
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.profiles.is_empty()
    }

    /// Returns the explicitly enabled profiles in command-line order.
    #[must_use]
    pub fn profiles(&self) -> &[ServerProfile] {
        &self.profiles
    }

    pub(crate) fn profile_for_path(&self, path: &RepoPath) -> Result<ServerProfile, LspError> {
        let extension = extension(path)
            .ok_or_else(|| {
                LspError::Disabled(format!(
                    "no enabled language server matches {}",
                    path.display()
                ))
            })?
            .to_ascii_lowercase();
        let matches = self
            .profiles
            .iter()
            .filter(|profile| profile.extensions.iter().any(|item| item == &extension))
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [] => Err(LspError::Disabled(format!(
                "no enabled language server matches .{extension} files"
            ))),
            [profile] => Ok((*profile).clone()),
            _ => Err(LspError::AmbiguousProfile(format!(
                "multiple enabled language servers match .{extension}; enable only one of: {}",
                matches
                    .iter()
                    .map(|profile| profile.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    #[serde(default)]
    servers: HashMap<String, RawProfile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProfile {
    language_id: String,
    extensions: Vec<String>,
    command: Vec<String>,
    #[serde(default)]
    root_markers: Vec<String>,
    initialization_options: Option<toml::Value>,
    #[serde(default)]
    workspace_data: bool,
    install_hint: Option<String>,
}

fn merge_user_profiles(
    path: &Path,
    source: &str,
    profiles: &mut HashMap<String, ServerProfile>,
) -> Result<(), LspConfigError> {
    let parsed: ConfigFile = toml::from_str(source)
        .map_err(|error| config_error(Some(path), format!("invalid TOML: {error}")))?;
    for (id, raw) in parsed.servers {
        let profile = validate_profile(id.clone(), raw)
            .map_err(|detail| config_error(Some(path), format!("profile {id:?}: {detail}")))?;
        profiles.insert(id, profile);
        if profiles.len() > MAX_PROFILES {
            return Err(config_error(Some(path), "too many LSP server profiles"));
        }
    }
    Ok(())
}

fn validate_profile(id: String, raw: RawProfile) -> Result<ServerProfile, String> {
    validate_identifier(&id, "profile id")?;
    validate_identifier(&raw.language_id, "language id")?;
    if raw.extensions.is_empty() || raw.extensions.len() > MAX_EXTENSIONS {
        return Err(format!(
            "extensions must contain 1 to {MAX_EXTENSIONS} values"
        ));
    }
    let mut extensions = HashSet::new();
    for extension in &raw.extensions {
        if extension.is_empty()
            || extension.starts_with('.')
            || !extension
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        {
            return Err(format!("invalid extension {extension:?}"));
        }
        if !extensions.insert(extension) {
            return Err(format!("duplicate extension {extension:?}"));
        }
    }
    validate_command(&raw.command)?;
    if raw
        .command
        .iter()
        .any(|part| matches!(part.as_str(), "{workspace_data}" | "{workspace_config}"))
        && !raw.workspace_data
    {
        return Err("workspace placeholders require workspace_data = true".to_owned());
    }
    if raw.root_markers.len() > MAX_ROOT_MARKERS {
        return Err(format!(
            "root_markers must contain at most {MAX_ROOT_MARKERS} values"
        ));
    }
    for marker in &raw.root_markers {
        validate_bounded(marker, "root marker")?;
        let marker_path = Path::new(marker);
        if marker_path.is_absolute()
            || marker_path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(format!("root marker {marker:?} must stay relative"));
        }
    }
    let initialization_options = raw
        .initialization_options
        .map(|value| serde_json::to_value(value).map_err(|error| error.to_string()))
        .transpose()?
        .unwrap_or_else(|| Value::Object(Default::default()));
    let install_hint = raw
        .install_hint
        .unwrap_or_else(|| format!("install the {id} language server and ensure it is on PATH"));
    validate_bounded(&install_hint, "install hint")?;
    Ok(ServerProfile {
        id,
        language_id: raw.language_id,
        extensions: raw.extensions,
        command: raw.command,
        root_markers: raw.root_markers,
        initialization_options,
        workspace_data: raw.workspace_data,
        install_hint,
    })
}

fn validate_command(command: &[String]) -> Result<(), String> {
    if command.is_empty() || command.len() > MAX_COMMAND_PARTS {
        return Err(format!(
            "command must contain 1 to {MAX_COMMAND_PARTS} arguments"
        ));
    }
    for part in command {
        validate_bounded(part, "command argument")?;
        if (part.contains('{') || part.contains('}'))
            && !matches!(
                part.as_str(),
                "{workspace_root}" | "{workspace_data}" | "{workspace_config}" | "{cache_dir}"
            )
        {
            return Err(format!("unsupported or partial placeholder in {part:?}"));
        }
    }
    let executable = Path::new(&command[0]);
    if matches!(command[0].as_str(), "." | "..") {
        return Err("executable must name a program".to_owned());
    }
    if !executable.is_absolute() && executable.components().count() != 1 {
        return Err("executable must be a bare PATH name or an absolute path".to_owned());
    }
    Ok(())
}

fn validate_identifier(value: &str, label: &str) -> Result<(), String> {
    validate_bounded(value, label)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(format!("{label} contains unsupported characters"));
    }
    Ok(())
}

fn validate_bounded(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_VALUE_BYTES || value.chars().any(char::is_control) {
        return Err(format!(
            "{label} must be non-empty, bounded, and contain no controls"
        ));
    }
    Ok(())
}

fn extension(path: &RepoPath) -> Option<&str> {
    let name = path.as_bytes().rsplit(|byte| *byte == b'/').next()?;
    let separator = name.iter().rposition(|byte| *byte == b'.')?;
    let extension = name.get(separator.saturating_add(1)..)?;
    std::str::from_utf8(extension).ok()
}

fn selected_profiles(
    enabled: &[String],
    profiles: &HashMap<String, ServerProfile>,
    path: Option<&Path>,
) -> Result<Vec<ServerProfile>, LspConfigError> {
    let mut selected = Vec::new();
    let mut seen = HashSet::new();
    for id in enabled {
        if !seen.insert(id.as_str()) {
            continue;
        }
        let profile = profiles.get(id).cloned().ok_or_else(|| {
            config_error(
                path,
                format!("unknown LSP profile {id:?}; choose a built-in or configured profile"),
            )
        })?;
        selected.push(profile);
    }
    Ok(selected)
}

fn default_path() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(value).join("chronogit/lsp.toml"));
    }
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|home| home.join(".config/chronogit/lsp.toml"))
}

fn config_error(path: Option<&Path>, detail: impl Into<String>) -> LspConfigError {
    LspConfigError {
        path: path.map(Path::to_path_buf),
        detail: detail.into(),
    }
}

fn builtin_profiles() -> Vec<ServerProfile> {
    let profile = |id: &str,
                   language_id: &str,
                   extensions: &[&str],
                   command: &[&str],
                   root_markers: &[&str],
                   workspace_data: bool,
                   install_hint: &str| ServerProfile {
        id: id.to_owned(),
        language_id: language_id.to_owned(),
        extensions: extensions.iter().map(|value| (*value).to_owned()).collect(),
        command: command.iter().map(|value| (*value).to_owned()).collect(),
        root_markers: root_markers
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        initialization_options: Value::Object(Default::default()),
        workspace_data,
        install_hint: install_hint.to_owned(),
    };
    vec![
        profile(
            "rust-analyzer",
            "rust",
            &["rs"],
            &["rust-analyzer"],
            &["Cargo.toml"],
            false,
            "install rust-analyzer and ensure rust-analyzer is on PATH",
        ),
        profile(
            "jdtls",
            "java",
            &["java"],
            &[
                "jdtls",
                "-configuration",
                "{workspace_config}",
                "-data",
                "{workspace_data}",
            ],
            &[
                "pom.xml",
                "build.gradle",
                "build.gradle.kts",
                "settings.gradle",
                ".project",
            ],
            true,
            "install Eclipse JDT LS (Java 21+ runtime) and ensure jdtls is on PATH",
        ),
        profile(
            "pyright",
            "python",
            &["py", "pyi"],
            &["pyright-langserver", "--stdio"],
            &["pyrightconfig.json", "pyproject.toml", "setup.cfg"],
            false,
            "install Pyright and ensure pyright-langserver is on PATH",
        ),
        profile(
            "basedpyright",
            "python",
            &["py", "pyi"],
            &["basedpyright-langserver", "--stdio"],
            &["pyrightconfig.json", "pyproject.toml"],
            false,
            "install basedpyright and ensure basedpyright-langserver is on PATH",
        ),
        profile(
            "pylsp",
            "python",
            &["py", "pyi"],
            &["pylsp"],
            &["pyproject.toml", "setup.cfg", "tox.ini"],
            false,
            "install python-lsp-server and ensure pylsp is on PATH",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::LspConfig;
    use crate::domain::{RepoPath, RepositoryRoot};

    #[test]
    fn builtins_are_opt_in_and_python_is_never_chosen_implicitly() {
        assert!(
            LspConfig::load(&[], None)
                .unwrap_or_else(|error| panic!("load: {error}"))
                .is_disabled()
        );
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp: {error}"));
        let config_path = directory.path().join("lsp.toml");
        fs::write(&config_path, "[servers]\n").unwrap_or_else(|error| panic!("config: {error}"));
        let config = LspConfig::load(
            &["pyright".to_owned(), "pylsp".to_owned()],
            Some(&config_path),
        )
        .unwrap_or_else(|error| panic!("load: {error}"));
        let path = RepoPath::from_bytes(b"src/main.py".to_vec())
            .unwrap_or_else(|error| panic!("path: {error}"));
        assert!(config.profile_for_path(&path).is_err());
    }

    #[test]
    fn disabled_mode_does_not_read_or_validate_lsp_configuration() {
        let missing = PathBuf::from("/definitely/missing/chronogit-lsp.toml");
        let config = LspConfig::load(&[], Some(&missing))
            .unwrap_or_else(|error| panic!("disabled LSP should ignore config: {error}"));
        assert!(config.is_disabled());
    }

    #[test]
    fn user_profile_is_data_driven_and_shell_free() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp: {error}"));
        let path = directory.path().join("lsp.toml");
        fs::write(
            &path,
            "[servers.gopls]\nlanguage_id = \"go\"\nextensions = [\"go\"]\ncommand = [\"gopls\"]\nroot_markers = [\"go.mod\"]\n",
        )
        .unwrap_or_else(|error| panic!("write: {error}"));
        let config = LspConfig::load(&["gopls".to_owned()], Some(&path))
            .unwrap_or_else(|error| panic!("load: {error}"));
        assert_eq!(config.profiles()[0].language_id(), "go");
    }

    #[test]
    fn rejects_partial_placeholders_parent_markers_and_unknown_fields() {
        for source in [
            "[servers.x]\nlanguage_id='x'\nextensions=['x']\ncommand=['server', '--cache={cache_dir}']\n",
            "[servers.x]\nlanguage_id='x'\nextensions=['x']\ncommand=['server', '{workspace_config}']\n",
            "[servers.x]\nlanguage_id='x'\nextensions=['x']\ncommand=['server']\nroot_markers=['../owned']\n",
            "[servers.x]\nlanguage_id='x'\nextensions=['x']\ncommand=['server']\nrepository_command=['bad']\n",
        ] {
            let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp: {error}"));
            let path = directory.path().join("lsp.toml");
            fs::write(&path, source).unwrap_or_else(|error| panic!("write: {error}"));
            assert!(LspConfig::load(&["x".to_owned()], Some(&path)).is_err());
        }
    }

    #[test]
    fn routes_rust_java_and_python_to_independent_nested_workspaces() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("temp: {error}"));
        for marker in ["rust/Cargo.toml", "java/pom.xml", "python/pyproject.toml"] {
            let marker = directory.path().join(marker);
            fs::create_dir_all(marker.parent().unwrap_or(directory.path()))
                .unwrap_or_else(|error| panic!("mkdir: {error}"));
            fs::write(marker, "").unwrap_or_else(|error| panic!("marker: {error}"));
        }
        let root = RepositoryRoot::new(
            fs::canonicalize(directory.path()).unwrap_or_else(|error| panic!("root: {error}")),
        )
        .unwrap_or_else(|error| panic!("root value: {error}"));
        let config_path = directory.path().join("lsp.toml");
        fs::write(&config_path, "[servers]\n").unwrap_or_else(|error| panic!("config: {error}"));
        let config = LspConfig::load(
            &[
                "rust-analyzer".to_owned(),
                "jdtls".to_owned(),
                "pyright".to_owned(),
            ],
            Some(&config_path),
        )
        .unwrap_or_else(|error| panic!("config: {error}"));
        for (path, id, workspace) in [
            ("rust/src/lib.rs", "rust-analyzer", "rust"),
            ("java/src/Main.java", "jdtls", "java"),
            ("python/pkg/main.py", "pyright", "python"),
        ] {
            let path = RepoPath::from_bytes(path.as_bytes().to_vec())
                .unwrap_or_else(|error| panic!("path: {error}"));
            let profile = config
                .profile_for_path(&path)
                .unwrap_or_else(|error| panic!("route: {error}"));
            assert_eq!(profile.id(), id);
            assert_eq!(
                profile.workspace_root(&root, &path),
                root.as_path().join(PathBuf::from(workspace))
            );
        }
    }
}
