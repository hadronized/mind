use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Node {
  /// Protocol version the tree is compatible with.
  #[serde(default)]
  version: Version,

  /// UID of the tree.
  uuid: String,

  /// Type of node.
  ///
  /// This is used to dispatch how to render and what actions are possible on a given node.
  #[serde(rename = "type")]
  ty: String,

  /// Icon associated with this node.
  icon: String,

  /// Whether the node is expanded or collapsed.
  is_expanded: bool,

  /// Text associated with the node.
  text: String,

  /// Children nodes, if any.
  children: Vec<Node>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Version(u16);

impl Default for Version {
  fn default() -> Self {
    Version(2)
  }
}
