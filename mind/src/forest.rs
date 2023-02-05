//! A forest is a set of trees, including:
//!
//! - A main tree.
//! - Project trees (cwd-based).

use crate::node::Tree;
use serde::{Deserialize, Serialize};
use std::{
  collections::HashMap,
  path::{Path, PathBuf},
};

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

  /// Get the main [`Tree`].
  pub fn main_tree(&self) -> &Tree {
    &self.tree
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
