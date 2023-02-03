use clap::{Parser, Subcommand, ValueEnum};
use std::{fmt::Display, path::PathBuf};

#[derive(Debug, Parser)]
pub struct CLI {
  /// Open a specific Mind tree at a given path.
  #[arg(short, long)]
  pub path: Option<PathBuf>,

  /// Use a CWD-tree instead of the global tree.
  #[arg(short, long, default_value_t = false)]
  pub cwd: bool,

  #[command(subcommand)]
  pub cmd: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
  /// Insert a new node.
  ///
  /// This command requires a base selection.
  #[command(alias = "ins")]
  Insert {
    #[arg(default_value_t, short, value_enum)]
    mode: InsertMode,

    /// Select a base node to operate on.
    sel: String,

    /// Name of the node to create.
    name: Vec<String>,
  },

  /// Remove a node
  ///
  /// This command requires a base selection.
  #[command(alias = "rm")]
  Remove {
    /// Select a base node to operate on.
    sel: String,
  },

  /// Rename a node.
  ///
  /// This command requires a base selection.
  Rename {
    /// Select a base node to operate on.
    sel: String,

    /// New name of the node.
    name: Vec<String>,
  },

  /// Change the icon of a node.
  ///
  /// This command requires a base selection
  Icon {
    /// Select a base node to operate on.
    sel: String,

    /// New icon of the node.
    icon: Vec<String>,
  },

  /// Move a node into another one.
  ///
  /// The selected node is the node to move and the path is the destination.
  #[command(alias = "mv")]
  Move {
    #[arg(default_value_t, short, value_enum)]
    mode: InsertMode,

    /// Select a base node to operate on.
    sel: String,

    /// Destination path
    dest: String,
  },

  /// Get all paths in a given node.
  Paths {
    /// Whether the paths should be printed to stdout.
    #[arg(short, long, default_value_t = true)]
    stdout: bool,

    /// Select a base node to operate on.
    sel: Option<String>,
  },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum InsertMode {
  /// Insert the node inside the selected node, at the top.
  #[value(name = "top")]
  InsideTop,

  /// Insert the node inside the selected node, at the bottom.
  #[default]
  #[value(name = "bottom")]
  InsideBottom,

  /// Insert the node as a sibling, just before the selected node (if the selected has a parent).
  Before,

  /// Insert the node as a sibling, just after the selected node (if the selected has a parent)
  After,
}

impl Display for InsertMode {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      InsertMode::InsideTop => f.write_str("top"),
      InsertMode::InsideBottom => f.write_str("bottom"),
      InsertMode::Before => f.write_str("before"),
      InsertMode::After => f.write_str("after"),
    }
  }
}
