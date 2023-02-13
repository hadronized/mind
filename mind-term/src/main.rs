mod cli;
mod config;

use clap::Parser;
use cli::{Command, DataCommand, DataType, InsertMode, CLI};
use colored::Colorize;
use config::Config;
use mind::forest::Forest;
use mind::node::{Node, NodeData, NodeError};
use mind::{encoding, node::Tree};
use std::borrow::Cow;
use std::env::current_dir;
use std::error::Error as StdError;
use std::fmt::Display;
use std::io::{read_to_string, stdin, stdout, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::{fs, io};
use thiserror::Error;

fn main() -> Result<(), Box<dyn StdError>> {
  let cli = CLI::parse();

  // TODO: get config from env var / XDG / whatever
  let (config, config_err) = Config::load_or_default();
  if let Some(config_err) = config_err {
    err_msg(format!("error while reading configuration: {}", config_err));
  }

  if let Some(ref path) = cli.path {
    // run on a specific Mind tree
    let tree: encoding::Tree =
      serde_json::from_str(&fs::read_to_string(path).map_err(PutainDeMerdeError::CannotReadTree)?)
        .map_err(PutainDeMerdeError::CannotDeserializeTree)?;
    let tree = Tree::from_encoding(tree);

    match with_tree(&config, cli, &tree)? {
      TreeFeedback::Persist => {
        // TODO: persist specific tree to path
      }

      TreeFeedback::Exit => (),
    }

    return Ok(());
  }

  let forest_path = config
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

  let feedback = if cli.cwd {
    // run by looking up the CWD
    let cwd = current_dir().map_err(PutainDeMerdeError::NoCWD)?;

    // TODO: check whether we want a local tree or a global one
    with_tree(
      &config,
      cli,
      forest
        .cwd_tree(&cwd)
        .ok_or_else(|| PutainDeMerdeError::NoCWDTree(cwd))?,
    )?
  } else {
    // use the main tree
    with_tree(&config, cli, forest.main_tree())?
  };

  match feedback {
    TreeFeedback::Persist => persist_forest(&forest, forest_path)?,
    TreeFeedback::Exit => (),
  }

  Ok(())
}

fn err_msg(msg: impl Display) {
  eprintln!("{}", msg.to_string().red());
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

  #[error("cannot get input from user")]
  UserInput(std::io::Error),
}

/// Feedback returned by operations dealing with trees.
///
/// The purpose of this type is to provide information to the caller about what to do next, mainly.
#[derive(Debug)]
enum TreeFeedback {
  Exit,
  Persist,
}

// TODO: extract the « interactive part » into a dedicated module / type.
fn get_base_sel(config: &Config, cli: &CLI, sel: &Option<String>, tree: &Tree) -> Option<Node> {
  sel
    .as_ref()
    .and_then(|path| tree.get_node_by_path(path_iter(&path)))
    .or_else(|| {
      // no explicit selection; try to use a fuzzy finder
      if !cli.interactive {
        return None;
      }

      let program = config.interactive.fuzzy_term_program()?;
      let child = std::process::Command::new(program)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
      let mut child_stdin = child.stdin?;
      write_paths("/", &tree.root(), &mut child_stdin).ok()?; // FIXME
      let path = read_to_string(&mut child.stdout?).ok()?; // FIXME

      if path.is_empty() {
        return None;
      }

      tree.get_node_by_path(path_iter(path.trim()))
    })
}

// TODO: extract the « interactive part » into a dedicated module / type.
fn get_input_string(prompt: impl AsRef<str>) -> Result<String, PutainDeMerdeError> {
  print!("{}", prompt.as_ref());
  stdout().flush().map_err(PutainDeMerdeError::UserInput)?;

  let mut input = String::new();
  let _ = stdin()
    .read_line(&mut input)
    .map_err(PutainDeMerdeError::UserInput)?;
  Ok(input)
}

fn with_tree(config: &Config, cli: CLI, tree: &Tree) -> Result<TreeFeedback, PutainDeMerdeError> {
  match &cli.cmd {
    Command::Insert { mode, sel, name } => {
      let sel =
        get_base_sel(config, &cli, &sel, tree).ok_or(PutainDeMerdeError::MissingBaseSelection)?;

      let name = match name {
        Some(name) => Cow::from(name),
        None => Cow::from(get_input_string("New node name > ")?),
      };

      insert(&sel, Node::new(name.trim(), ""), *mode)?;
      Ok(TreeFeedback::Persist)
    }

    Command::Remove { sel } => {
      let sel =
        get_base_sel(config, &cli, &sel, tree).ok_or(PutainDeMerdeError::MissingBaseSelection)?;
      remove(sel)?;
      Ok(TreeFeedback::Persist)
    }

    Command::Rename { sel, name } => {
      let sel =
        get_base_sel(config, &cli, &sel, tree).ok_or(PutainDeMerdeError::MissingBaseSelection)?;

      let name = match name {
        Some(name) => Cow::from(name),
        None => Cow::from(get_input_string("Rename node > ")?),
      };

      rename(sel, name.trim())?;
      Ok(TreeFeedback::Persist)
    }

    Command::Icon { sel, icon } => {
      let sel =
        get_base_sel(config, &cli, &sel, tree).ok_or(PutainDeMerdeError::MissingBaseSelection)?;

      let icon = match icon {
        Some(icon) => Cow::from(icon),
        None => Cow::from(get_input_string("Change node icon > ")?),
      };

      change_icon(sel, icon.trim());
      Ok(TreeFeedback::Persist)
    }

    Command::Move { mode, sel, dest } => {
      let sel =
        get_base_sel(config, &cli, &sel, tree).ok_or(PutainDeMerdeError::MissingBaseSelection)?;

      let dest =
        get_base_sel(config, &cli, &dest, tree).ok_or(PutainDeMerdeError::MissingBaseSelection)?;

      move_from_to(sel, dest, *mode)?;
      Ok(TreeFeedback::Persist)
    }

    Command::Paths { sel } => {
      let prefix = sel.as_deref().unwrap_or("/");
      let sel = tree
        .get_node_by_path(path_iter(prefix))
        .ok_or(PutainDeMerdeError::MissingBaseSelection)?;
      write_paths(prefix, &sel, &mut io::stdout())?;

      Ok(TreeFeedback::Exit)
    }

    Command::Data { sel, ty, cmd } => {
      let sel =
        get_base_sel(config, &cli, &sel, tree).ok_or(PutainDeMerdeError::MissingBaseSelection)?;

      match cmd {
        DataCommand::Get => {
          if let Some(content) = sel.data() {
            match (ty, content) {
              (DataType::File, NodeData::File(path)) => println!("{}", path.display()),
              (DataType::Link, NodeData::Link(link)) => println!("{}", link),
              _ => Err(NodeError::MismatchDataType)?,
            }
          }

          Ok(TreeFeedback::Exit)
        }

        DataCommand::Set { content } => {
          let data = match ty {
            DataType::File => NodeData::file(content),
            DataType::Link => NodeData::link(content),
          };
          sel.set_data(data)?;

          Ok(TreeFeedback::Persist)
        }
      }
    }
  }
}

/// Write paths to the provided writer.
fn write_paths(
  prefix: &str,
  base_sel: &Node,
  writer: &mut impl Write,
) -> Result<(), PutainDeMerdeError> {
  for path in base_sel.paths(prefix) {
    writeln!(writer, "{}", path).map_err(PutainDeMerdeError::CannotWritePath)?;
  }

  Ok(())
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

fn path_iter<'a>(path: &'a str) -> impl Iterator<Item = &'a str> {
  path.split('/').filter(|frag| !frag.trim().is_empty())
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
