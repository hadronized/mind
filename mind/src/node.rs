//! Node operations

use crate::encoding::{self, TreeType};
use std::{
  cell::RefCell,
  ops::{Deref, DerefMut},
  rc::Rc,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tree {
  version: encoding::Version,
  ty: TreeType,
  node: Node,
}

impl Tree {
  pub fn from_encoding(tree: encoding::Tree) -> Self {
    Self {
      version: tree.version,
      ty: tree.ty,
      node: Node::from_encoding(tree.node),
    }
  }

  /// Get a [`Node`] by line number.
  ///
  /// 0-indexed.
  pub fn get_node_by_line(&self, line: usize) -> Option<Node> {
    let (_, node) = self.node.get_node_by_line(line);
    node
  }

  /// Get a [`Node`] by path, e.g. `/root/a/b/c/d`.
  pub fn get_node_by_path<'a>(&self, path: impl IntoIterator<Item = &'a str>) -> Option<Node> {
    self.node.get_node_by_path(path.into_iter())
  }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Node {
  inner: Rc<RefCell<NodeInner>>,
}

impl Node {
  fn new(icon: String, is_expanded: bool, name: String, parent: Option<Node>) -> Self {
    Self {
      inner: Rc::new(RefCell::new(NodeInner {
        icon,
        is_expanded,
        name,
        parent,
        children: Vec::new(),
      })),
    }
  }

  #[cfg(test)]
  fn new_by_expand_state(name: impl Into<String>, is_expanded: bool, children: Vec<Node>) -> Self {
    Self {
      inner: Rc::new(RefCell::new(NodeInner {
        icon: String::new(),
        is_expanded,
        name: name.into(),
        parent: None,
        children,
      })),
    }
  }

  pub fn from_encoding(node: encoding::Node) -> Self {
    Self::from_encoding_rec(None, node)
  }

  fn from_encoding_rec(parent: Option<Node>, mut node: encoding::Node) -> Self {
    let current = Self::new(
      node.icon,
      node.is_expanded,
      node
        .contents
        .pop()
        .map(|text| text.text)
        .unwrap_or_default(),
      parent,
    );

    let children = node
      .children
      .into_iter()
      .map(|node| Self::from_encoding_rec(Some(current.clone()), node))
      .collect();

    current.borrow_mut().children = children;
    current
  }

  pub fn get_node_by_line(&self, mut line: usize) -> (usize, Option<Self>) {
    let node = self.inner.borrow();

    if line == 0 {
      return (0, Some(self.clone()));
    }

    // jump the current node
    line -= 1;

    if !node.is_expanded || node.children.is_empty() {
      return (line, None);
    }

    for child in &node.children {
      let (new_line, node) = child.get_node_by_line(line);
      if node.is_some() {
        return (new_line, node);
      }

      line = new_line;
    }

    (line, None)
  }

  fn get_node_by_path<'a>(&self, mut path: impl Iterator<Item = &'a str>) -> Option<Self> {
    let node = self.inner.borrow();

    match path.next() {
      None => Some(self.clone()),

      Some(node_name) => {
        // find the node in the children list, and if it doesn’t exist, it means the node we are looking for doesn’t exist;
        // abord early
        node
          .children
          .iter()
          .find(|node| node.borrow().name == node_name)?
          .get_node_by_path(path)
      }
    }
  }
}

impl Deref for Node {
  type Target = Rc<RefCell<NodeInner>>;

  fn deref(&self) -> &Self::Target {
    &self.inner
  }
}

impl DerefMut for Node {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.inner
  }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeInner {
  icon: String,
  is_expanded: bool,
  name: String,
  parent: Option<Node>,
  children: Vec<Node>,
}

#[cfg(test)]
mod tests {
  use crate::{
    encoding::{TreeType, Version},
    node::{Node, Tree},
  };

  #[test]
  fn get_node_by_line_no_child() {
    let tree = Tree {
      version: Version::default(),
      ty: TreeType::Root,
      node: Node::new_by_expand_state("root", false, vec![]),
    };

    assert_eq!(
      tree
        .get_node_by_line(0)
        .as_ref()
        .map(|node| node.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("root")
    );
    assert_eq!(tree.get_node_by_line(1), None);
    assert_eq!(tree.get_node_by_line(2), None);

    let tree = Tree {
      version: Version::default(),
      ty: TreeType::Root,
      node: Node::new_by_expand_state("root", true, vec![]),
    };

    assert_eq!(
      tree
        .get_node_by_line(0)
        .as_ref()
        .map(|node| node.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("root")
    );
    assert_eq!(
      tree
        .get_node_by_line(0)
        .as_ref()
        .map(|node| node.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("root")
    );
    assert_eq!(tree.get_node_by_line(1), None);
    assert_eq!(tree.get_node_by_line(2), None);
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
    let tree = Tree {
      version: Version::default(),
      ty: TreeType::Root,
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
      tree
        .get_node_by_line(0)
        .as_ref()
        .map(|node| node.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("root")
    );
    assert_eq!(
      tree
        .get_node_by_line(1)
        .as_ref()
        .map(|node| node.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("a")
    );
    assert_eq!(
      tree
        .get_node_by_line(2)
        .as_ref()
        .map(|node| node.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("b")
    );
    assert_eq!(
      tree
        .get_node_by_line(3)
        .as_ref()
        .map(|node| node.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("z")
    );
    assert_eq!(
      tree
        .get_node_by_line(4)
        .as_ref()
        .map(|node| node.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("c")
    );
  }

  #[test]
  fn get_node_by_path_no_child() {
    let tree = Tree {
      version: Version::default(),
      ty: TreeType::Root,
      node: Node::new_by_expand_state("root", false, vec![]),
    };

    assert_eq!(
      tree
        .get_node_by_line(0)
        .as_ref()
        .map(|node| node.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("root")
    );
    assert_eq!(tree.get_node_by_path(["test"]), None);

    let tree = Tree {
      version: Version::default(),
      ty: TreeType::Root,
      node: Node::new_by_expand_state("root", true, vec![]),
    };

    assert_eq!(
      tree
        .get_node_by_line(0)
        .as_ref()
        .map(|node| node.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("root")
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
    let tree = Tree {
      version: Version::default(),
      ty: TreeType::Root,
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
      tree
        .get_node_by_line(0)
        .as_ref()
        .map(|node| node.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("root")
    );
    assert_eq!(
      tree
        .get_node_by_path(["a"])
        .as_ref()
        .map(|node| node.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("a")
    );
    assert_eq!(
      tree
        .get_node_by_path(["a", "x"])
        .as_ref()
        .map(|node| node.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("x")
    );
    assert_eq!(
      tree
        .get_node_by_path(["a", "y"])
        .as_ref()
        .map(|node| node.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("y")
    );
    assert_eq!(
      tree
        .get_node_by_path(["b"])
        .as_ref()
        .map(|node| node.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("b")
    );
    assert_eq!(
      tree
        .get_node_by_path(["b", "z"])
        .as_ref()
        .map(|node| node.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("z")
    );
    assert_eq!(
      tree
        .get_node_by_path(["c"])
        .as_ref()
        .map(|node| node.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("c")
    );
  }
}
