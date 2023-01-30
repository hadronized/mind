mod cli;

use clap::Parser;
use cli::{Command, InsertMode, CLI};
use mind::node::{Node, NodeError};
use mind::{encoding, node::Tree};
use std::error::Error as StdError;
use std::fs;
use thiserror::Error;

fn main() -> Result<(), Box<dyn StdError>> {
  let cli = CLI::parse();

  // run on a specific Mind tree
  if let Some(ref path) = cli.path {
    let tree: encoding::Tree = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let tree = Tree::from_encoding(tree);
    with_tree(cli, tree)?;
  }

  Ok(())
}

#[derive(Debug, Error)]
pub enum PutainDeMerdeError {
  #[error("missing a base node selection")]
  MissingBaseSelection,

  #[error("forbidden node operation")]
  NodeOperation(#[from] NodeError),
}

fn with_tree(cli: CLI, tree: Tree) -> Result<(), Box<dyn StdError>> {
  let base_sel = cli
    .base_sel
    .as_ref()
    .and_then(|base_sel| tree.get_node_by_path(path_iter(base_sel)));

  match cli.cmd {
    Command::Insert { mode, name } => {
      let base_sel = base_sel.ok_or(PutainDeMerdeError::MissingBaseSelection)?;
      let name = name.join(" ");
      insert(&base_sel, Node::new(name, ""), mode)?;
    }

    Command::Remove => {
      let base_sel = base_sel.ok_or(PutainDeMerdeError::MissingBaseSelection)?;
      remove(base_sel)?;
    }

    Command::Rename { name } => {
      let base_sel = base_sel.ok_or(PutainDeMerdeError::MissingBaseSelection)?;
      let name = name.join(" ");
      rename(base_sel, name)?;
    }

    Command::Icon { icon } => {
      let base_sel = base_sel.ok_or(PutainDeMerdeError::MissingBaseSelection)?;
      let icon = icon.join(" ");
      change_icon(base_sel, icon);
    }

    Command::Move { mode, dest } => {
      let base_sel = base_sel.ok_or(PutainDeMerdeError::MissingBaseSelection)?;
      let dest = tree
        .get_node_by_path(path_iter(&dest))
        .ok_or(PutainDeMerdeError::MissingBaseSelection)?;
      move_from_to(base_sel, dest, mode)?;
    }

    Command::Paths { stdout } => {
      let base_sel = base_sel.unwrap_or_else(|| tree.root());
      if stdout {
        for path in base_sel.paths() {
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
