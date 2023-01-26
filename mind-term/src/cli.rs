use std::{fmt::Display, path::PathBuf, str::FromStr};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct CLI {
  /// Path to a Mind tree.
  #[structopt(short, long)]
  pub path: Option<PathBuf>,

  /// Select a base node to operate on.
  #[structopt(short = "s", long = "sel")]
  pub base_sel: Option<String>,

  #[structopt(subcommand)]
  pub cmd: Command,
}

#[derive(Debug, StructOpt)]
pub enum Command {
  /// Insert a new node.
  ///
  /// This command requires a base selection.
  #[structopt(aliases = &["ins"])]
  Insert {
    #[structopt(default_value, short)]
    mode: InsertMode,

    /// Name of the node to create.
    name: Vec<String>,
  },
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum InsertMode {
  /// Insert the node inside the selected node, at the top.
  InsideTop,

  /// Insert the node inside the selected node, at the bottom.
  #[default]
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

#[derive(Debug)]
pub struct InsertModeParseError;

impl Display for InsertModeParseError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("FOCK")
  }
}

impl FromStr for InsertMode {
  type Err = InsertModeParseError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "top" => Ok(InsertMode::InsideTop),
      "bottom" => Ok(InsertMode::InsideBottom),
      "before" => Ok(InsertMode::Before),
      "after" => Ok(InsertMode::After),
      _ => Err(InsertModeParseError),
    }
  }
}
