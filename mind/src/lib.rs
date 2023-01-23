use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
  /// Get a [`Node`] by line number.
  ///
  /// 0-indexed.
  pub fn get_node_by_line_nb(&mut self, line: usize) -> Option<&mut Node> {
    let (_, node) = self.node.get_node_by_line_nb(line);
    node
  }

  /// Get a [`Node`] by path, e.g. `/root/a/b/c/d`.
  pub fn get_node_by_path<'a>(
    &mut self,
    path: impl IntoIterator<Item = &'a str>,
  ) -> Option<&mut Node> {
    self.node.get_node_by_path(path.into_iter())
  }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

impl Node {
  pub fn new_by_expand_state(
    text: impl Into<String>,
    is_expanded: bool,
    children: impl IntoIterator<Item = Node>,
  ) -> Self {
    Self {
      icon: String::new(),
      is_expanded,
      contents: vec![Text { text: text.into() }],
      children: children.into_iter().collect(),
    }
  }

  pub fn set_name(&mut self, name: impl Into<String>) -> String {
    let prev = self
      .contents
      .pop()
      .map(|Text { text }| text)
      .unwrap_or_default();
    self.contents.push(Text { text: name.into() });
    prev
  }

  fn get_node_by_line_nb(&mut self, mut line: usize) -> (usize, Option<&mut Self>) {
    if line == 0 {
      return (0, Some(self));
    }

    // jump the current node
    line -= 1;

    if !self.is_expanded || self.children.is_empty() {
      return (line, None);
    }

    for child in &mut self.children {
      let (new_line, node) = child.get_node_by_line_nb(line);
      if node.is_some() {
        return (new_line, node);
      }

      line = new_line;
    }

    (line, None)
  }

  fn get_node_by_path<'a>(&mut self, mut path: impl Iterator<Item = &'a str>) -> Option<&mut Self> {
    match path.next() {
      None => Some(self),

      Some(node_name) => {
        // find the node in the children list, and if it doesn’t exist, it means the node we are looking for doesn’t exist;
        // abord early
        self
          .children
          .iter_mut()
          .find(|node| {
            node
              .contents
              .first()
              .map(|text| text.text == node_name)
              .unwrap_or(false)
          })?
          .get_node_by_path(path)
      }
    }
  }
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

#[cfg(test)]
mod tests {
  use crate::{Node, Text, Tree, Version};

  #[test]
  fn get_node_by_line_no_child() {
    let mut tree = Tree {
      version: Version::default(),
      ty: 0,
      node: Node::new_by_expand_state("root", false, vec![]),
    };

    assert_eq!(
      tree.get_node_by_line_nb(0).map(|node| &node.contents),
      Some(&vec![Text {
        text: "root".to_owned()
      }])
    );
    assert_eq!(tree.get_node_by_line_nb(1), None);
    assert_eq!(tree.get_node_by_line_nb(2), None);

    let mut tree = Tree {
      version: Version::default(),
      ty: 0,
      node: Node::new_by_expand_state("root", true, vec![]),
    };

    assert_eq!(
      tree.get_node_by_line_nb(0).map(|node| &node.contents),
      Some(&vec![Text {
        text: "root".to_owned()
      }])
    );
    assert_eq!(tree.get_node_by_line_nb(1), None);
    assert_eq!(tree.get_node_by_line_nb(2), None);
  }

  // this tests a couple of queries on this tree:
  //
  // root/       expanded     line:0
  //   a/        collapsed    line:1
  //     x/
  //     y/
  //   b/        expanded     line:2
  //     z/
  //   c/
  #[test]
  fn get_node_by_line_with_children() {
    let mut tree = Tree {
      version: Version::default(),
      ty: 0,
      node: Node::new_by_expand_state(
        "root",
        true,
        vec![
          Node::new_by_expand_state(
            "a",
            false,
            vec![
              Node::new_by_expand_state("x", false, vec![]),
              Node::new_by_expand_state("y", false, vec![]),
            ],
          ),
          Node::new_by_expand_state(
            "b",
            true,
            vec![Node::new_by_expand_state("z", false, vec![])],
          ),
          Node::new_by_expand_state("c", false, vec![]),
        ],
      ),
    };

    assert_eq!(
      tree.get_node_by_line_nb(0).map(|node| &node.contents),
      Some(&vec![Text {
        text: "root".to_owned()
      }])
    );
    assert_eq!(
      tree.get_node_by_line_nb(1).map(|node| &node.contents),
      Some(&vec![Text {
        text: "a".to_owned()
      }])
    );
    assert_eq!(
      tree.get_node_by_line_nb(2).map(|node| &node.contents),
      Some(&vec![Text {
        text: "b".to_owned()
      }])
    );
    assert_eq!(
      tree.get_node_by_line_nb(3).map(|node| &node.contents),
      Some(&vec![Text {
        text: "z".to_owned()
      }])
    );
    assert_eq!(
      tree.get_node_by_line_nb(4).map(|node| &node.contents),
      Some(&vec![Text {
        text: "c".to_owned()
      }])
    );
  }

  #[test]
  fn get_node_by_path_no_child() {
    let mut tree = Tree {
      version: Version::default(),
      ty: 0,
      node: Node::new_by_expand_state("root", false, vec![]),
    };

    assert_eq!(
      tree.get_node_by_line_nb(0).map(|node| &node.contents),
      Some(&vec![Text {
        text: "root".to_owned()
      }])
    );
    assert_eq!(tree.get_node_by_path(["test"]), None);

    let mut tree = Tree {
      version: Version::default(),
      ty: 0,
      node: Node::new_by_expand_state("root", true, vec![]),
    };

    assert_eq!(
      tree.get_node_by_line_nb(0).map(|node| &node.contents),
      Some(&vec![Text {
        text: "root".to_owned()
      }])
    );
    assert_eq!(tree.get_node_by_path(["test"]), None);
  }

  // this tests a couple of queries on this tree:
  //
  // root/       expanded     line:0
  //   a/        collapsed    line:1
  //     x/
  //     y/
  //   b/        expanded     line:2
  //     z/
  //   c/
  #[test]
  fn get_node_by_path_with_children() {
    let mut tree = Tree {
      version: Version::default(),
      ty: 0,
      node: Node::new_by_expand_state(
        "root",
        true,
        vec![
          Node::new_by_expand_state(
            "a",
            false,
            vec![
              Node::new_by_expand_state("x", false, vec![]),
              Node::new_by_expand_state("y", false, vec![]),
            ],
          ),
          Node::new_by_expand_state(
            "b",
            true,
            vec![Node::new_by_expand_state("z", false, vec![])],
          ),
          Node::new_by_expand_state("c", false, vec![]),
        ],
      ),
    };

    assert_eq!(
      tree.get_node_by_line_nb(0).map(|node| &node.contents),
      Some(&vec![Text {
        text: "root".to_owned()
      }])
    );
    assert_eq!(
      tree.get_node_by_path(["a"]).map(|node| &node.contents),
      Some(&vec![Text {
        text: "a".to_owned()
      }])
    );
    assert_eq!(
      tree.get_node_by_path(["a", "x"]).map(|node| &node.contents),
      Some(&vec![Text {
        text: "x".to_owned()
      }])
    );
    assert_eq!(
      tree.get_node_by_path(["a", "y"]).map(|node| &node.contents),
      Some(&vec![Text {
        text: "y".to_owned()
      }])
    );
    assert_eq!(
      tree.get_node_by_path(["b"]).map(|node| &node.contents),
      Some(&vec![Text {
        text: "b".to_owned()
      }])
    );
    assert_eq!(
      tree.get_node_by_path(["b", "z"]).map(|node| &node.contents),
      Some(&vec![Text {
        text: "z".to_owned()
      }])
    );
    assert_eq!(
      tree.get_node_by_path(["c"]).map(|node| &node.contents),
      Some(&vec![Text {
        text: "c".to_owned()
      }])
    );
  }
}
