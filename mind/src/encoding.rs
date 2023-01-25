//! Encoding representation of trees and nodes

use serde::{de::Error as _, Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeType {
  /// A root.
  Root = 0,

  /// A local root.
  Local = 1,
}

impl Serialize for TreeType {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    (*self as u8).serialize(serializer)
  }
}

impl<'de> Deserialize<'de> for TreeType {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: serde::Deserializer<'de>,
  {
    match u8::deserialize(deserializer)? {
      0 => Ok(TreeType::Root),
      1 => Ok(TreeType::Local),
      _ => Err(D::Error::custom("nope")),
    }
  }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Tree {
  /// Protocol version the tree is compatible with.
  #[serde(default)]
  pub version: Version,

  /// Type of node.
  #[serde(rename = "type")]
  pub ty: TreeType,

  /// A tree is also a node, so we flatten the content of a node when doing deser.
  #[serde(flatten)]
  pub node: Node,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Node {
  /// Icon associated with this node.
  #[serde(default)]
  pub(crate) icon: String,

  /// Whether the node is expanded or collapsed.
  #[serde(default)]
  pub(crate) is_expanded: bool,

  /// Text associated with the node.
  pub(crate) contents: Vec<Text>,

  /// Children nodes, if any.
  #[serde(default)]
  pub(crate) children: Vec<Node>,
}

impl Node {
  #[cfg(test)]
  pub(crate) fn new_by_expand_state(
    name: impl Into<String>,
    is_expanded: bool,
    children: Vec<Node>,
  ) -> Self {
    Self {
      icon: String::new(),
      is_expanded,
      contents: vec![Text { text: name.into() }],
      children,
    }
  }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Version(u16);

impl Version {
  pub const fn current() -> Self {
    Version(1)
  }
}

impl Default for Version {
  fn default() -> Self {
    Version::current()
  }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Text {
  pub text: String,
}
