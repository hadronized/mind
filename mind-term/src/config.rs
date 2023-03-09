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

  #[serde(default)]
  pub ui: UIConfig,

  #[serde(default)]
  pub tree: TreeConfig,
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
      .or_else(|| dirs::data_dir().map(|p| p.join("mind/data")))
  }

  pub fn forest_path(&self) -> Option<PathBuf> {
    self
      .state_path
      .clone()
      .or_else(|| dirs::data_dir().map(|p| p.join("mind/mind.json")))
  }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct InteractiveConfig {
  /// Fuzzy finder to use in terminal mode.
  fuzzy_term_program: Option<String>,

  /// Switch / option name to set the prompt. The prompt will be passed after the switch. So if you set this option
  /// to e.g. `--prompt`, we will pass `--prompt "Actual prompt"` to your fuzzy program.
  fuzzy_term_prompt_opt: Option<String>,
}

impl InteractiveConfig {
  pub fn fuzzy_term_program(&self) -> Option<&str> {
    self.fuzzy_term_program.as_deref()
  }

  pub fn fuzzy_term_prompt_opt(&self) -> Option<&str> {
    self.fuzzy_term_prompt_opt.as_deref()
  }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct UIConfig {
  pub editor: Option<String>,
  pub extension: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TreeConfig {
  /// Whether nodes should be automatically created when selected if they don’t exist yet.
  pub auto_create_nodes: bool,
}
