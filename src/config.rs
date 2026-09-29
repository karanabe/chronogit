//! User-level display settings loaded before terminal initialization.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Default number of context rows above and below the selected row.
pub const DEFAULT_SCROLLOFF: usize = 2;

/// Display settings shared by lists and text panes.
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    scrolloff: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scrolloff: DEFAULT_SCROLLOFF,
        }
    }
}

impl Config {
    /// Loads `--config` or the optional user-level `chronogit/config.toml`.
    ///
    /// Uses `XDG_CONFIG_HOME`, falling back to `~/.config`. An absent implicit
    /// file uses defaults; an explicit file must exist.
    ///
    /// # Errors
    /// Returns a path-qualified error for unreadable files or invalid TOML.
    pub fn load(path: Option<&Path>) -> Result<Self, ConfigError> {
        let explicit = path.is_some();
        let path = path.map(Path::to_path_buf).or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME")
                        .filter(|value| !value.is_empty())
                        .map(|home| PathBuf::from(home).join(".config"))
                })
                .map(|directory| directory.join("chronogit/config.toml"))
        });
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) if !explicit && error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(ConfigError {
                    path,
                    detail: error.to_string(),
                });
            }
        };
        toml::from_str(&source).map_err(|error| ConfigError {
            path,
            detail: error.to_string(),
        })
    }

    /// Returns the requested context rows; zero disables the margin.
    #[must_use]
    pub fn scrolloff(&self) -> usize {
        self.scrolloff
    }
}

/// A path-qualified display configuration failure.
#[derive(Debug)]
pub struct ConfigError {
    path: PathBuf,
    detail: String,
}

impl Display for ConfigError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "could not load config {}: {}",
            self.path.display(),
            self.detail
        )
    }
}

impl Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn loads_an_explicit_file_and_reports_its_path_on_failure() {
        let directory = tempfile::tempdir().unwrap_or_else(|error| panic!("{error}"));
        let path = directory.path().join("config.toml");
        assert!(Config::load(Some(&path)).is_err());
        std::fs::write(&path, "scrolloff = 3").unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            Config::load(Some(&path))
                .unwrap_or_else(|error| panic!("{error}"))
                .scrolloff(),
            3
        );
        std::fs::write(&path, "scrolloff = -1").unwrap_or_else(|error| panic!("{error}"));
        let error = Config::load(Some(&path))
            .err()
            .unwrap_or_else(|| panic!("invalid config was accepted"));
        assert!(error.to_string().contains(&path.display().to_string()));
    }

    #[test]
    fn parses_scrolloff_and_rejects_invalid_settings() {
        for (source, expected) in [("", 2), ("scrolloff = 0", 0), ("scrolloff = 3", 3)] {
            let config: Config = toml::from_str(source).unwrap_or_else(|error| panic!("{error}"));
            assert_eq!(config.scrolloff(), expected);
        }
        for source in [
            "scrolloff = -1",
            "scrolloff = '2'",
            "scrolloff = 2.5",
            "scroloff = 2",
        ] {
            assert!(toml::from_str::<Config>(source).is_err(), "{source}");
        }
    }
}
