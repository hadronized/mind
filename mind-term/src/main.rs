mod cli;

use cli::{Command, InsertMode, CLI};
use mind::node::{Node, NodeError};
use mind::{encoding, node::Tree};
use std::error::Error as StdError;
use std::fs;
use structopt::StructOpt;
use thiserror::Error;

fn main() -> Result<(), Box<dyn StdError>> {
  let config = CLI::from_args();

  // run on a specific Mind tree
  if let Some(ref path) = config.path {
    let tree: encoding::Tree = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let tree = Tree::from_encoding(tree);
    with_tree(&config, tree)?;
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

fn with_tree(config: &CLI, tree: Tree) -> Result<(), Box<dyn StdError>> {
  let base_sel = config.base_sel.as_ref().and_then(|base_sel| {
    tree.get_node_by_path(base_sel.split('/').filter(|frag| !frag.trim().is_empty()))
  });

  match config.cmd {
    Command::Insert { mode, ref name } => {
      let base_sel = base_sel.ok_or(PutainDeMerdeError::MissingBaseSelection)?;
      let name = name.join(" ");

      insert(&base_sel, Node::new(name, ""), mode)?;
    }
  }

  println!("{tree:#?}");

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
