mod config;

use config::Config;
use mind::{encoding, node::Tree};
use std::fs;
use structopt::StructOpt;

fn main() {
  let config = Config::from_args();

  // run on a specific Mind tree
  if let Some(ref path) = config.path {
    let tree: encoding::Tree = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    let tree = Tree::from_encoding(tree);
    with_tree(&config, tree);
  }
}

fn with_tree(config: &Config, tree: Tree) {
  let base_sel = config
    .base_sel
    .as_ref()
    .map(|base_sel| base_sel.split('/').filter(|frag| !frag.trim().is_empty()));

  if let Some(base_sel) = base_sel {
    let node = tree.get_node_by_path(base_sel);
    println!("selected node: {node:#?}");
  }
}
