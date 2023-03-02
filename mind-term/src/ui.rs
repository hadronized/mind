//! User Interface types and functions

use mind::node::{path_iter, Node, NodeFilter, Tree};
use std::{
  io::{self, read_to_string, stdin, stdout, Write},
  process::Stdio,
};
use thiserror::Error;

#[derive(Debug)]
pub struct UI {
  fuzzy_term_program: Option<String>,
}

impl UI {
  pub fn new(fuzzy_term_program: Option<String>) -> Self {
    Self { fuzzy_term_program }
  }

  pub fn get_base_sel(
    &self,
    interactive: bool,
    sel: &Option<String>,
    filter: NodeFilter,
    tree: &Tree,
  ) -> Option<Node> {
    {
      sel
        .as_ref()
        .and_then(|path| tree.get_node_by_path(path_iter(&path)))
        .or_else(|| {
          // no explicit selection; try to use a fuzzy finder
          if !interactive {
            return None;
          }

          let program = self.fuzzy_term_program.as_ref()?;
          let child = std::process::Command::new(program)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .ok()?;
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
}

#[derive(Debug, Error)]
pub enum UIError {
  #[error("cannot get user input: {0}")]
  UserInput(io::Error),
}
