use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Tree {
  /// Protocol version the tree is compatible with.
  #[serde(default)]
  version: Version,

  /// Type of node.
  #[serde(rename = "type")]
  ty: u8,

  /// A tree is also a node, so we flatten the content of a node when doing deser.
  #[serde(flatten)]
  node: Node,
}

impl Tree {
  pub fn get_node_by_line_nb(&self, line: usize) -> Option<&Node> {
    todo!()
  }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Node {
  /// Icon associated with this node.
  #[serde(default)]
  icon: String,

  /// Whether the node is expanded or collapsed.
  #[serde(default)]
  is_expanded: bool,

  /// Text associated with the node.
  contents: Vec<Text>,

  /// Children nodes, if any.
  #[serde(default)]
  children: Vec<Node>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Version(u16);

impl Default for Version {
  fn default() -> Self {
    Version(1)
  }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Text {
  text: String,
}
