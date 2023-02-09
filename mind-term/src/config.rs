//! Main configuration of the CLI.

use serde::{Deserialize, Serialize};
use std::{fs::read_to_string, path::PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
  #[error("no configuration path")]
  NoConfigPath,

  #[error("cannot read the configuration file")]
  CannotRead {
    #[source]
    #[from]
    err: std::io::Error,
  },

  #[error("cannot deserialize the configuration file")]
  CannotDeserialize {
    #[source]
    #[from]
    err: toml::de::Error,
  },
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Config {
  #[serde(default)]
  pub persistence: PersistenceConfig,

  #[serde(default)]
  pub interactive: InteractiveConfig,
}

impl Config {
  pub fn path() -> Result<PathBuf, ConfigError> {
    dirs::config_dir()
      .map(|d| d.join("mind/config.toml"))
      .ok_or(ConfigError::NoConfigPath)
  }

  /// Load the [`Config`] from [`Config::path`].
  pub fn load() -> Result<Self, ConfigError> {
    let path = Self::path()?;
    let contents = read_to_string(path)?;
    Ok(toml::from_str(&contents)?)
  }

  /// Load the [`Config`] from [`Config::path`] or use the default one.
  pub fn load_or_default() -> (Self, Option<ConfigError>) {
    Self::load()
      .map(|config| (config, None))
      .unwrap_or_else(|e| (Self::default(), Some(e)))
  }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PersistenceConfig {
  /// Directory where to store data.
  ///
  /// Defaults to `$XDG_DATA_HOME/mind/data`.
  data_dir: Option<PathBuf>,

  /// Path to the forest.
  ///
  /// Defaults to `$XDG_DATA_HOME/mind/state.json`.
  state_path: Option<PathBuf>,
}

impl PersistenceConfig {
  pub fn data_dir(&self) -> Option<PathBuf> {
    self
      .data_dir
      .clone()
      .or(dirs::data_dir().map(|p| p.join("mind/data")))
  }

  pub fn forest_path(&self) -> Option<PathBuf> {
    self
      .state_path
      .clone()
      .or(dirs::data_dir().map(|p| p.join("mind/mind.json")))
  }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct InteractiveConfig {
  /// Fuzzy finder to use in terminal mode.
  fuzzy_term_program: Option<String>,
}

impl InteractiveConfig {
  pub fn fuzzy_term_program(&self) -> Option<&str> {
    self.fuzzy_term_program.as_deref()
  }
}
