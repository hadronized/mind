//! User Interface types and functions

use crate::config::Config;
use mind_tree::node::{NodeFilter, Tree};
use std::{
  io::{self, read_to_string, stdin, stdout, Write},
  path::Path,
  process::Stdio,
};
use thiserror::Error;

#[derive(Debug)]
pub struct UI {
  fuzzy_term_program: Option<String>,
  fuzzy_term_prompt_opt: Option<String>,
  editor: Option<String>,
}

impl UI {
  pub fn new(config: &Config) -> Self {
    Self {
      fuzzy_term_program: config.interactive.fuzzy_term_program().map(Into::into),
      fuzzy_term_prompt_opt: config.interactive.fuzzy_term_prompt_opt().map(Into::into),
      editor: config.ui.editor.clone(),
    }
  }

  pub fn select_path(
    &self,
    picker_opts: PickerOptions,
    filter: NodeFilter,
    tree: &Tree,
  ) -> Option<String> {
    let PickerOptions::Interactive { prompt } = picker_opts else { return None; };
    let program = self.fuzzy_term_program.as_ref()?;
    let mut child = std::process::Command::new(program);
    child.stdin(Stdio::piped()).stdout(Stdio::piped());

    if let Some(ref prompt_prefix) = self.fuzzy_term_prompt_opt {
      child.arg(format!("{} {}", prompt_prefix, prompt));
    }

    let child = child.spawn().ok()?;
    let mut child_stdin = child.stdin?;
    tree
      .root()
      .write_paths("/", filter, &mut child_stdin)
      .ok()?; // FIXME: muted error?!
    read_to_string(&mut child.stdout?).ok().and_then(|s| {
      // FIXME: muted error, do we really like that?
      let s = s.trim();

      if s.is_empty() {
        None
      } else {
        Some(s.to_owned())
      }
    })
  }

  pub fn input(&self, picker_opts: PickerOptions) -> Option<String> {
    let PickerOptions::Interactive { prompt } = picker_opts else { return None; };

    print!("{}", prompt);
    stdout().flush().ok()?;

    let mut input = String::new();
    let _ = stdin().read_line(&mut input).ok()?;
    Some(input)
  }

  /// Get the editor name.
  fn get_editor(&self) -> Result<String, UIError> {
    self
      .editor
      .as_ref()
      .cloned()
      .or_else(|| std::env::var("EDITOR").ok())
      .ok_or(UIError::NoEditor)
  }

  /// Open the editor at the given path.
  pub fn open_with_editor(&self, path: impl AsRef<Path>) -> Result<(), UIError> {
    let editor = self.get_editor()?;
    std::process::Command::new(editor)
      .arg(path.as_ref())
      .status()
      .map(|_| ())
      .map_err(UIError::EditorError)
  }

  pub fn open_uri(&self, uri: impl AsRef<str>) -> Result<(), UIError> {
    let uri = uri.as_ref();
    open::that(uri).map_err(|err| UIError::URIError {
      uri: uri.to_owned(),
      err,
    })
  }
}

#[derive(Debug, Error)]
pub enum UIError {
  #[error("cannot get user input: {0}")]
  UserInput(io::Error),

  #[error("no editor configured; either set the $EDITOR environment variable or the edit.editor configuration path")]
  NoEditor,

  #[error("error while editing: process returned {0}")]
  EditorError(io::Error),

  #[error("error while opening URI {uri}: {err}")]
  URIError { uri: String, err: io::Error },
}

#[derive(Debug)]
pub enum PickerOptions {
  NonInteractive,
  Interactive { prompt: &'static str },
}

impl PickerOptions {
  /// Check whether we want an interactive picker. If we do, use the provided prompt.
  pub fn either(interactive: bool, prompt: &'static str) -> Self {
    if interactive {
      Self::Interactive { prompt }
    } else {
      Self::NonInteractive
    }
  }
}
