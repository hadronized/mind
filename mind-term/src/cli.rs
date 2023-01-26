use clap::{Parser, Subcommand, ValueEnum};
use std::{fmt::Display, path::PathBuf};

#[derive(Debug, Parser)]
pub struct CLI {
  /// Path to a Mind tree.
  #[arg(short, long)]
  pub path: Option<PathBuf>,

  /// Select a base node to operate on.
  #[arg(short = 's', long)]
  pub base_sel: Option<String>,

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

    /// Name of the node to create.
    name: Vec<String>,
  },

  /// Remove a node
  ///
  /// This command requires a base selection.
  #[command(alias = "rm")]
  Remove,
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
