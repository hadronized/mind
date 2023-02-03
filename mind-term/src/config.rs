//! Main configuration of the CLI.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Config {
  pub persistence: PersistenceConfig,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

  pub fn state_path(&self) -> Option<PathBuf> {
    self
      .state_path
      .clone()
      .or(dirs::data_dir().map(|p| p.join("mind/state.json")))
  }
}
