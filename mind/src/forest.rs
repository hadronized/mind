//! A forest is a set of trees, including:
//!
//! - A main tree.
//! - Project trees (cwd-based).

use crate::node::Tree;
use serde::{Deserialize, Serialize};
use std::{
  collections::HashMap,
  fs,
  path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize)]
pub struct Forest {
  /// The main tree.
  tree: Tree,

  /// CWD-based trees.
  ///
  /// The keys are absolute paths.
  projects: HashMap<PathBuf, Tree>,
}

impl Forest {
  /// Create a new empty [`Forest`].
  ///
  /// This function creates an empty main tree with the provided `name` and `icon`, and no CWD-based project is
  /// initialized.
  pub fn new(tree: Tree) -> Self {
    Self {
      tree,
      projects: HashMap::new(),
    }
  }

  /// Load the forest from a path.
  pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ForestError> {
    let path = path.as_ref();

    if !path.exists() {
      return Err(ForestError::NotPersisted(path.to_owned()));
    }

    let contents = fs::read_to_string(path).map_err(ForestError::CannotReadFromFS)?;
    serde_json::from_str(&contents).map_err(ForestError::CannotDeserialize)
  }

  pub fn persist(&self, path: impl AsRef<Path>) -> Result<(), ForestError> {
    let path = path.as_ref();

    // ensure all parent directories are created
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent).map_err(ForestError::CannotWriteToFS)?;
    }

    let serialized = serde_json::to_string(self).map_err(ForestError::CannotSerialize)?;
    fs::write(path, serialized).map_err(ForestError::CannotWriteToFS)?;
    Ok(())
  }

  /// Get the main [`Tree`].
  pub fn main_tree(&self) -> &Tree {
    &self.tree
  }

  /// Return all the trees with their corresponding CWD.
  pub fn cwd_trees(&self) -> impl Iterator<Item = (&Path, &Tree)> {
    self
      .projects
      .iter()
      .map(|(cwd, tree)| (cwd.as_path(), tree))
  }

  /// Get a CWD-based [`Tree`].
  pub fn cwd_tree(&self, cwd: impl AsRef<Path>) -> Option<&Tree> {
    self.projects.get(cwd.as_ref())
  }

  /// Add a [`Tree`] for the given CWD.
  pub fn add_cwd_tree(&mut self, cwd: impl Into<PathBuf>, tree: Tree) {
    let _ = self.projects.insert(cwd.into(), tree);
  }
}

#[derive(Debug, Error)]
pub enum ForestError {
  #[error("no forest persisted at path {0}")]
  NotPersisted(PathBuf),

  #[error("cannot read forest from the file system: {0}")]
  CannotReadFromFS(std::io::Error),

  #[error("cannot write forest to the file system: {0}")]
  CannotWriteToFS(std::io::Error),

  #[error("cannot deserialize forest: {0}")]
  CannotDeserialize(serde_json::error::Error),

  #[error("cannot serialize forest: {0}")]
  CannotSerialize(serde_json::error::Error),
}
