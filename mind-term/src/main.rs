mod cli;
mod config;

use clap::Parser;
use cli::{Command, InsertMode, CLI};
use colored::Colorize;
use config::Config;
use mind::forest::Forest;
use mind::node::{Node, NodeError};
use mind::{encoding, node::Tree};
use std::error::Error as StdError;
use std::fmt::Display;
use std::fs;
use std::path::Path;
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
    let tree: encoding::Tree = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let tree = Tree::from_encoding(tree);
    return Ok(with_tree(cli, tree)?);
  }

  let forest = load_forest(&config).or_else(|e| match e {
    PutainDeMerdeError::NoForestPersisted => {
      // no forest persisted yet, create a new one…
      let forest = Forest::new(Tree::new("Main", " "));

      // … and persist it
      let path = config
        .persistence
        .forest_path()
        .ok_or(PutainDeMerdeError::NoForestPath)?;
      persist_forest(&forest, path)?;

      Ok(forest)
    }

    _ => Err(e),
  })?;

  println!("{forest:#?}");

  if cli.cwd {
    // run by looking up the CWD
    // TODO: check whether we want a local tree or a global one
    todo!()
  } else {
    // use the main tree
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
}

fn with_tree(cli: CLI, tree: Tree) -> Result<(), Box<dyn StdError>> {
  match cli.cmd {
    Command::Insert { mode, sel, name } => {
      let sel = tree
        .get_node_by_path(path_iter(&sel))
        .ok_or(PutainDeMerdeError::MissingBaseSelection)?;
      let name = name.join(" ");
      insert(&sel, Node::new(name, ""), mode)?;
    }

    Command::Remove { sel } => {
      let sel = tree
        .get_node_by_path(path_iter(&sel))
        .ok_or(PutainDeMerdeError::MissingBaseSelection)?;
      remove(sel)?;
    }

    Command::Rename { sel, name } => {
      let sel = tree
        .get_node_by_path(path_iter(&sel))
        .ok_or(PutainDeMerdeError::MissingBaseSelection)?;
      let name = name.join(" ");
      rename(sel, name)?;
    }

    Command::Icon { sel, icon } => {
      let sel = tree
        .get_node_by_path(path_iter(&sel))
        .ok_or(PutainDeMerdeError::MissingBaseSelection)?;
      let icon = icon.join(" ");
      change_icon(sel, icon);
    }

    Command::Move { mode, sel, dest } => {
      let sel = tree
        .get_node_by_path(path_iter(&sel))
        .ok_or(PutainDeMerdeError::MissingBaseSelection)?;
      let dest = tree
        .get_node_by_path(path_iter(&dest))
        .ok_or(PutainDeMerdeError::MissingBaseSelection)?;
      move_from_to(sel, dest, mode)?;
    }

    Command::Paths { stdout, sel } => {
      let path = sel.as_deref().unwrap_or("/");
      let sel = tree
        .get_node_by_path(path_iter(path))
        .ok_or(PutainDeMerdeError::MissingBaseSelection)?;
      if stdout {
        let prefix = if path.starts_with("/") { "" } else { "/" };
        println!("{prefix}{path}");
        for path in sel.paths() {
          println!("{}", path);
        }
      }
    }
  }

  Ok(())
}

/// Insert a node into a selected one.
fn insert(base_sel: &Node, node: Node, mode: InsertMode) -> Result<(), PutainDeMerdeError> {
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

fn load_forest(config: &Config) -> Result<Forest, PutainDeMerdeError> {
  let forest_path = config
    .persistence
    .forest_path()
    .ok_or(PutainDeMerdeError::NoForestPath)?;

  if forest_path.exists() {
    let contents =
      std::fs::read_to_string(forest_path).map_err(PutainDeMerdeError::CannotReadForest)?;
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
