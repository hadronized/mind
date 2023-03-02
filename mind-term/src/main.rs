mod cli;
mod config;
mod ui;

use clap::Parser;
use cli::{Command, DataCommand, DataType, InsertMode, CLI};
use colored::Colorize;
use config::Config;
use mind::forest::Forest;
use mind::node::{Node, NodeData, NodeError, NodeFilter};
use mind::{encoding, node::Tree};
use std::borrow::Cow;
use std::env::current_dir;
use std::path::{Path, PathBuf};
use std::{fs, io};
use thiserror::Error;
use ui::{UIError, UI};

/// The top-level type holding everything that the application is about.
struct App {
  config: Config,
  ui: UI,
}

impl App {
  fn new() -> Self {
    let config = Self::load_config();
    let ui = UI::new(config.interactive.fuzzy_term_program().map(Into::into));

    Self { config, ui }
  }

  fn load_config() -> Config {
    // TODO: get config from env var / XDG / whatever
    let (config, config_err) = Config::load_or_default();
    if let Some(config_err) = config_err {
      eprintln!(
        "{}",
        format!("error while reading configuration: {}", config_err).red()
      );
    }

    config
  }

  /// Start the application by adding an error handler layer.
  fn with_error_handler(self) {
    match self.run() {
      Err(err) => {
        eprintln!("{}", err.to_string().red());
      }

      _ => (),
    }
  }

  /// Start and dispatch the application by looking at the CLI, config, etc.
  fn run(self) -> Result<(), PutainDeMerdeError> {
    let cli = CLI::parse();

    // check if we are running on a specific path
    if let Some(ref path) = cli.path {
      self.run_specific_tree(cli.interactive, cli.cmd, path)?;
      return Ok(());
    }

    // we are running config-based
    self.run_config_based(cli.interactive, cli.cmd, cli.cwd)?;

    Ok(())
  }

  fn run_specific_tree(
    &self,
    interactive: bool,
    cmd: Command,
    path: &Path,
  ) -> Result<(), PutainDeMerdeError> {
    let tree: encoding::Tree =
      serde_json::from_str(&fs::read_to_string(path).map_err(PutainDeMerdeError::CannotReadTree)?)
        .map_err(PutainDeMerdeError::CannotDeserializeTree)?;
    let tree = Tree::from_encoding(tree);

    match self.dispatch_cmd(interactive, cmd, &tree)? {
      TreeFeedback::Persist => {
        // TODO: persist specific tree to path
      }

      TreeFeedback::Exit => (),
    }

    Ok(())
  }

  fn run_config_based(
    &self,
    interactive: bool,
    cmd: Command,
    cwd: bool,
  ) -> Result<(), PutainDeMerdeError> {
    let forest_path = self
      .config
      .persistence
      .forest_path()
      .ok_or(PutainDeMerdeError::NoForestPath)?;

    let forest = load_forest(&forest_path).or_else(|e| match e {
      PutainDeMerdeError::NoForestPersisted => {
        // no forest persisted yet, create a new one…
        let forest = Forest::new(Tree::new("Main", " "));

        // … and persist it
        persist_forest(&forest, &forest_path)?;

        Ok(forest)
      }

      _ => Err(e),
    })?;

    let tree = if cwd {
      // run by looking up the CWD
      let cwd = current_dir().map_err(PutainDeMerdeError::NoCWD)?;
      forest
        .cwd_tree(&cwd)
        .ok_or_else(|| PutainDeMerdeError::NoCWDTree(cwd))?
    } else {
      forest.main_tree()
    };

    // TODO: check whether we want a local tree or a global one
    let feedback = self.dispatch_cmd(interactive, cmd, tree)?;
    match feedback {
      TreeFeedback::Persist => persist_forest(&forest, forest_path)?,
      TreeFeedback::Exit => (),
    }

    Ok(())
  }

  fn dispatch_cmd(
    &self,
    interactive: bool,
    cmd: Command,
    tree: &Tree,
  ) -> Result<TreeFeedback, PutainDeMerdeError> {
    match cmd {
      Command::Insert { mode, sel, name } => {
        self.run_insert_cmd(interactive, tree, mode, sel, name)
      }

      Command::Remove { sel } => self.run_remove_cmd(interactive, tree, sel),

      Command::Rename { sel, name } => self.run_rename_cmd(interactive, tree, sel, name),

      Command::Icon { sel, icon } => self.run_icon_cmd(interactive, tree, sel, icon),

      Command::Move { mode, sel, dest } => self.run_move_cmd(interactive, tree, mode, sel, dest),

      // TODO: add filtering
      Command::Paths { sel, ty } => self.run_paths_cmd(interactive, tree, sel, ty),

      Command::Data { sel, ty, cmd } => self.run_data_cmd(interactive, tree, sel, ty, cmd),
    }
  }

  fn run_insert_cmd(
    &self,
    interactive: bool,
    tree: &Tree,
    mode: InsertMode,
    sel: Option<String>,
    name: Option<String>,
  ) -> Result<TreeFeedback, PutainDeMerdeError> {
    let sel = self
      .ui
      .get_base_sel(interactive, &sel, NodeFilter::default(), tree)
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    let name = match name {
      Some(name) => Cow::from(name),
      None => Cow::from(self.ui.get_input_string("New node name > ")?),
    };

    insert(&sel, Node::new(name.trim(), ""), mode)?;
    Ok(TreeFeedback::Persist)
  }

  fn run_remove_cmd(
    &self,
    interactive: bool,
    tree: &Tree,
    sel: Option<String>,
  ) -> Result<TreeFeedback, PutainDeMerdeError> {
    let sel = self
      .ui
      .get_base_sel(interactive, &sel, NodeFilter::default(), tree)
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;
    remove(sel)?;
    Ok(TreeFeedback::Persist)
  }

  fn run_rename_cmd(
    &self,
    interactive: bool,
    tree: &Tree,
    sel: Option<String>,
    name: Option<String>,
  ) -> Result<TreeFeedback, PutainDeMerdeError> {
    let sel = self
      .ui
      .get_base_sel(interactive, &sel, NodeFilter::default(), tree)
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    let name = match name {
      Some(name) => Cow::from(name),
      None => Cow::from(self.ui.get_input_string("Rename node > ")?),
    };

    rename(sel, name.trim())?;
    Ok(TreeFeedback::Persist)
  }

  fn run_icon_cmd(
    &self,
    interactive: bool,
    tree: &Tree,
    sel: Option<String>,
    icon: Option<String>,
  ) -> Result<TreeFeedback, PutainDeMerdeError> {
    let sel = self
      .ui
      .get_base_sel(interactive, &sel, NodeFilter::default(), tree)
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    let icon = match icon {
      Some(icon) => Cow::from(icon),
      None => Cow::from(self.ui.get_input_string("Change node icon > ")?),
    };

    change_icon(sel, icon.trim());
    Ok(TreeFeedback::Persist)
  }

  fn run_move_cmd(
    &self,
    interactive: bool,
    tree: &Tree,
    mode: InsertMode,
    sel: Option<String>,
    dest: Option<String>,
  ) -> Result<TreeFeedback, PutainDeMerdeError> {
    let sel = self
      .ui
      .get_base_sel(interactive, &sel, NodeFilter::default(), tree)
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    let dest = self
      .ui
      .get_base_sel(interactive, &dest, NodeFilter::default(), tree)
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    move_from_to(sel, dest, mode)?;
    Ok(TreeFeedback::Persist)
  }

  fn run_paths_cmd(
    &self,
    interactive: bool,
    tree: &Tree,
    sel: Option<String>,
    ty: Option<DataType>,
  ) -> Result<TreeFeedback, PutainDeMerdeError> {
    let prefix = sel.as_deref().unwrap_or("/");

    let filter = ty.map(DataType::to_filter).unwrap_or_default();

    let sel = self
      .ui
      .get_base_sel(interactive, &sel, NodeFilter::default(), tree)
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    sel.write_paths(prefix, filter, &mut io::stdout())?;

    Ok(TreeFeedback::Exit)
  }

  fn run_data_cmd(
    &self,
    interactive: bool,
    tree: &Tree,
    sel: Option<String>,
    ty: Option<DataType>,
    cmd: DataCommand,
  ) -> Result<TreeFeedback, PutainDeMerdeError> {
    let filter = ty.map(DataType::to_filter).unwrap_or_default();
    let sel = self
      .ui
      .get_base_sel(interactive, &sel, filter, tree)
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    match cmd {
      DataCommand::Get => {
        if let Some(content) = sel.data() {
          match (ty, content) {
            (None | Some(DataType::File), NodeData::File(path)) => {
              println!("{}", path.display())
            }
            (None | Some(DataType::Link), NodeData::Link(link)) => println!("{}", link),
            _ => Err(NodeError::MismatchDataType)?,
          }
        }

        Ok(TreeFeedback::Exit)
      }

      DataCommand::Set { content } => {
        let data = ty
          .map(|ty| match ty {
            DataType::File => NodeData::file(&content),
            DataType::Link => NodeData::link(&content),
          })
          .unwrap_or_else(|| {
            if content.is_empty() {
              // TODO: we need to create a file / something here
              NodeData::file(content)
            } else {
              NodeData::link(content)
            }
          });
        sel.set_data(data)?;

        Ok(TreeFeedback::Persist)
      }
    }
  }
}

fn main() {
  let app = App::new();
  app.with_error_handler();
}

#[derive(Debug, Error)]
pub enum PutainDeMerdeError {
  #[error("missing a base node selection")]
  MissingBaseSelection,

  #[error("forbidden node operation")]
  NodeOperation(#[from] NodeError),

  #[error("no forest path; are you running without a filesystem?")]
  NoForestPath,

  #[error("no forest persisted yet")]
  NoForestPersisted,

  #[error("error while serializing forest")]
  CannotSerializeForest(serde_json::Error),

  #[error("error while deserializing forest")]
  CannotDeserializeForest(serde_json::Error),

  #[error("error while creating directories to hold the forest on the filesystem")]
  CannotCreateForestDirectories(std::io::Error),

  #[error("error while writing forest to the filesystem")]
  CannotWriteForest(std::io::Error),

  #[error("error while reading forest from the filesystem")]
  CannotReadForest(std::io::Error),

  #[error("error while serializing specific tree from the filesystem")]
  CannotSerializeTree(serde_json::Error),

  #[error("error while deserializing specific tree from the filesystem")]
  CannotDeserializeTree(serde_json::Error),

  #[error("error while reading specific tree from the filesystem")]
  CannotReadTree(std::io::Error),

  #[error("error while writing specific tree from the filesystem")]
  CannotWriteTree(std::io::Error),

  #[error("no current working directory")]
  NoCWD(std::io::Error),

  #[error("no such CWD-based tree")]
  NoCWDTree(PathBuf),

  #[error("node with empty name")]
  EmptyName,

  #[error("cannot write a path")]
  CannotWritePath(io::Error),

  #[error("UI error: {0}")]
  UIError(#[from] UIError),
}

/// Feedback returned by operations dealing with trees.
///
/// The purpose of this type is to provide information to the caller about what to do next, mainly.
#[derive(Debug)]
enum TreeFeedback {
  Exit,
  Persist,
}

/// Insert a node into a selected one.
fn insert(base_sel: &Node, node: Node, mode: InsertMode) -> Result<(), PutainDeMerdeError> {
  if node.name().is_empty() {
    return Err(PutainDeMerdeError::EmptyName);
  }

  match mode {
    InsertMode::InsideTop => base_sel.insert_top(node),
    InsertMode::InsideBottom => base_sel.insert_bottom(node),
    InsertMode::Before => base_sel.insert_before(node)?,
    InsertMode::After => base_sel.insert_after(node)?,
  }

  Ok(())
}

/// Delete a node.
fn remove(base_sel: Node) -> Result<(), PutainDeMerdeError> {
  let parent = base_sel.parent()?;
  Ok(parent.delete(base_sel)?)
}

/// Rename a node.
fn rename(base_sel: Node, name: impl AsRef<str>) -> Result<(), PutainDeMerdeError> {
  let name = name.as_ref();

  if name.is_empty() {
    return Err(PutainDeMerdeError::EmptyName);
  }

  Ok(base_sel.set_name(name)?)
}

/// Change the icon of a node
fn change_icon(base_sel: Node, icon: impl AsRef<str>) {
  base_sel.set_icon(icon);
}

/// Move a node from a source to a destination.
fn move_from_to(src: Node, dest: Node, mode: InsertMode) -> Result<(), PutainDeMerdeError> {
  match mode {
    InsertMode::InsideTop => Ok(dest.move_top(src)?),
    InsertMode::InsideBottom => Ok(dest.move_bottom(src)?),
    InsertMode::Before => Ok(dest.move_before(src)?),
    InsertMode::After => Ok(dest.move_after(src)?),
  }
}

fn load_forest(path: impl AsRef<Path>) -> Result<Forest, PutainDeMerdeError> {
  let path = path.as_ref();

  if path.exists() {
    let contents = std::fs::read_to_string(path).map_err(PutainDeMerdeError::CannotReadForest)?;
    serde_json::from_str(&contents).map_err(PutainDeMerdeError::CannotDeserializeForest)
  } else {
    // nothing to load
    Err(PutainDeMerdeError::NoForestPersisted)
  }
}

fn persist_forest(forest: &Forest, path: impl AsRef<Path>) -> Result<(), PutainDeMerdeError> {
  let path = path.as_ref();

  // ensure all parent directories are created
  match path.parent() {
    Some(parent) => {
      std::fs::create_dir_all(parent).map_err(PutainDeMerdeError::CannotCreateForestDirectories)?;
    }
    _ => (),
  }

  let serialized =
    serde_json::to_string(forest).map_err(PutainDeMerdeError::CannotSerializeForest)?;
  std::fs::write(path, serialized).map_err(PutainDeMerdeError::CannotWriteForest)?;
  Ok(())
}
