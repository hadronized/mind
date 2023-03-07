//! User Interface types and functions

use crate::config::Config;
use mind::node::{path_iter, Node, NodeFilter, Tree};
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
      editor: config.edit.editor.clone(),
    }
  }

  pub fn get_base_sel(
    &self,
    picker_opts: PickerOptions,
    sel: Option<&str>,
    filter: NodeFilter,
    tree: &Tree,
  ) -> Option<Node> {
    {
      sel
        .and_then(|path| tree.get_node_by_path(path_iter(&path)))
        .or_else(|| {
          let prompt = match picker_opts {
            // no explicit selection; try to use a fuzzy finder
            PickerOptions::NonInteractive => return None,
            PickerOptions::Interactive { prompt } => prompt,
          };

          let program = self.fuzzy_term_program.as_ref()?;
          let mut child = std::process::Command::new(program);
          child.stdin(Stdio::piped()).stdout(Stdio::piped());

          match self.fuzzy_term_prompt_opt {
            Some(ref prompt_prefix) => {
              child.arg(format!("{} {}", prompt_prefix, prompt));
            }

            _ => (),
          }

          let child = child.spawn().ok()?;
          let mut child_stdin = child.stdin?;
          tree
            .root()
            .write_paths("/", filter, &mut child_stdin)
            .ok()?; // FIXME: muted error?!
          let path = read_to_string(&mut child.stdout?).ok()?; // FIXME: muted error, do we really like that?

          if path.is_empty() {
            return None;
          }

          tree.get_node_by_path(path_iter(path.trim()))
        })
    }
  }

  pub fn get_input_string(&self, prompt: impl AsRef<str>) -> Result<String, UIError> {
    print!("{}", prompt.as_ref());
    stdout().flush().map_err(UIError::UserInput)?;

    let mut input = String::new();
    let _ = stdin().read_line(&mut input).map_err(UIError::UserInput)?;
    Ok(input)
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
    open::that(&uri).map_err(|err| UIError::URIError {
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
