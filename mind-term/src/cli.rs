use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
  author = "Dimitri Sabadie <dimitri.sabadie@gmail.com>",
  name = "mind",
  version,
  about = "Organize your thoughts in a tree-like structure"
)]
pub struct Cli {
  #[command(subcommand)]
  pub cmd: Command,
}

/// Common arguments used by most actions.
#[derive(Args, Debug)]
pub struct CommonArgs {
  #[arg(short, long)]
  pub path: Option<PathBuf>,

  /// Use a CWD-tree instead of the global tree.
  #[arg(short, long)]
  pub cwd: bool,

  /// Use a local tree.
  ///
  /// This implies --path and --cwd, so you don’t have to set them.
  #[arg(short, long)]
  pub local: bool,

  /// Interactive mode.
  ///
  /// When run in interactive mode, base selections can be selected via a fuzzy program.
  #[arg(short, long)]
  pub interactive: bool,
}

/// Data-oriented arguments.
#[derive(Args, Debug)]
pub struct DataArgs {
  /// Associate a file with the node.
  #[arg(short, long)]
  pub file: bool,

  /// Associate a URI with the node.
  #[arg(short, long)]
  pub uri: Option<Option<String>>,

  /// Open a node if it contains data.
  ///
  /// “Opening” is contextual: if the node is a file node, the file will be edited with your editor (either via the
  /// $EDITOR environment variable, or via the edit.editor configuration path). If it’s a link node, a command used
  /// to open URI will be used, depending on your operating system.
  #[arg(short, long)]
  pub open: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
  /// Initialize a new Mind tree.
  Init {
    #[clap(flatten)]
    common_args: CommonArgs,

    /// Name of the tree. Can be changed later.
    name: Option<String>,
  },

  /// Insert a new node.
  ///
  /// This command requires a base selection.
  #[command(alias = "ins")]
  Insert {
    /// Use a specific Mind tree at a given path.
    #[clap(flatten)]
    common_args: CommonArgs,

    #[arg(default_value_t, short, long, value_enum)]
    mode: InsertMode,

    #[clap(flatten)]
    data_args: DataArgs,

    /// Source node.
    #[arg(short, long)]
    source: Option<String>,

    /// Name of the node to create.
    #[arg(short, long)]
    name: Option<String>,
  },

  /// Remove a node
  ///
  /// This command requires a base selection.
  #[command(alias = "rm")]
  Remove {
    #[clap(flatten)]
    common_args: CommonArgs,

    /// Select a base node to operate on.
    #[arg(short, long)]
    source: Option<String>,
  },

  /// Rename a node.
  ///
  /// This command requires a base selection.
  Rename {
    #[clap(flatten)]
    common_args: CommonArgs,

    /// Node to rename.
    #[arg(short, long)]
    source: Option<String>,

    /// New name of the node.
    #[arg(short, long)]
    new: Option<String>,
  },

  /// Change the icon of a node.
  ///
  /// This command requires a base selection
  Icon {
    #[command(flatten)]
    common_args: CommonArgs,

    /// Node to change the icon.
    #[arg(short, long)]
    source: Option<String>,

    /// New icon of the node.
    #[arg(short, long)]
    text: Option<String>,
  },

  /// Move a node into another one.
  ///
  /// The selected node is the node to move and the path is the destination.
  #[command(alias = "mv")]
  Move {
    #[clap(flatten)]
    common_args: CommonArgs,

    #[arg(default_value_t, short, value_enum)]
    mode: InsertMode,

    /// Source path.
    #[arg(short, long)]
    source: Option<String>,

    /// Destination path.
    #[arg(short, long)]
    dest: Option<String>,
  },

  /// Get all paths in a given node.
  Paths {
    #[command(flatten)]
    common_args: CommonArgs,

    /// Filter by file nodes.
    #[arg(short, long)]
    file: bool,

    /// Filter by URI nodes.
    #[arg(short, long)]
    uri: bool,

    /// Select a base node to operate on.
    #[arg(short, long)]
    source: Option<String>,
  },

  /// Get associated data with a node.
  Get {
    #[clap(flatten)]
    common_args: CommonArgs,

    /// Filter by file nodes.
    #[arg(short, long)]
    file: bool,

    /// Filter by URI nodes.
    #[arg(short, long)]
    uri: bool,

    /// Open a node if it contains data.
    ///
    /// “Opening” is contextual: if the node is a file node, the file will be edited with your editor (either via the
    /// $EDITOR environment variable, or via the edit.editor configuration path). If it’s a link node, a command used
    /// to open URI will be used, depending on your operating system.
    #[arg(short, long)]
    open: bool,

    /// Select a base node to operate on.
    #[arg(short, long)]
    source: Option<String>,
  },

  /// Associate data to a node.
  Set {
    #[clap(flatten)]
    common_args: CommonArgs,

    #[clap(flatten)]
    data_args: DataArgs,

    /// Select a base node to operate on.
    #[arg(short, long)]
    source: Option<String>,
  },

  /// List all the currently known trees.
  #[command(name = "ls")]
  List {},

  /// Run the TUI (mind-tui), if installed.
  Tui {
    #[clap(flatten)]
    common_args: CommonArgs,
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
