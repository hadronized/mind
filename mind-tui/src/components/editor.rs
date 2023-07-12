use std::{path::Path, process::Command};

use mind_tree::config::Config;

use crate::error::AppError;

#[derive(Debug)]
pub struct Editor {
  cmd: String,
}

impl Editor {
  pub fn new(config: &Config) -> Result<Self, AppError> {
    // get the editor to use to open the file
    let cmd = config
      .ui
      .editor
      .as_ref()
      .cloned()
      .or_else(|| std::env::var("EDITOR").ok())
      .ok_or_else(|| AppError::EditorConfig {
        err: "no editor configured".to_owned(),
      })?;

    Ok(Editor { cmd })
  }

  /// Open the given path with the editor.
  pub fn edit(&self, path: &Path) -> Result<(), AppError> {
    Command::new(&self.cmd)
      .arg(path)
      .status()
      .map_err(|err| AppError::NodePathOpenError {
        path: path.to_owned(),
        err: format!("error while opening editor: {}", err),
      })?;

    Ok(())
  }
}
