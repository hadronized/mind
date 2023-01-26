//! Node operations

use crate::encoding::{self, TreeType};
use std::{
  cell::RefCell,
  rc::{Rc, Weak},
};
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct Tree {
  version: encoding::Version,
  ty: TreeType,
  node: Node,
}

impl Tree {
  pub fn new(name: impl Into<String>, icon: impl Into<String>) -> Self {
    Self {
      version: encoding::Version::current(),
      ty: TreeType::Root,
      node: Node::new_raw(name.into(), icon.into(), false, None),
    }
  }

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

/// Weak version of [`Node`], mainly for parent nodes.
///
/// Not supposed to be used as-is; convert to [`Node`] when needed.
#[derive(Clone, Debug)]
pub struct WeakNode {
  inner: Weak<RefCell<NodeInner>>,
}

impl WeakNode {
  fn upgrade(&self) -> Option<Node> {
    self.inner.upgrade().map(|inner| Node { inner })
  }
}

#[derive(Clone, Debug)]
pub struct Node {
  inner: Rc<RefCell<NodeInner>>,
}

impl PartialEq for Node {
  fn eq(&self, other: &Self) -> bool {
    self.inner.as_ptr().eq(&other.inner.as_ptr())
  }
}

impl Node {
  pub fn new(name: impl Into<String>, icon: impl Into<String>) -> Self {
    Self::new_raw(name.into(), icon.into(), false, None)
  }

  fn new_raw(name: String, icon: String, is_expanded: bool, parent: Option<WeakNode>) -> Self {
    Self {
      inner: Rc::new(RefCell::new(NodeInner {
        name,
        icon,
        is_expanded,
        parent,
        children: Vec::new(),
      })),
    }
  }

  fn downgrade(&self) -> WeakNode {
    WeakNode {
      inner: Rc::downgrade(&self.inner),
    }
  }

  pub fn from_encoding(node: encoding::Node) -> Self {
    Self::from_encoding_rec(None, node)
  }

  fn from_encoding_rec(parent: Option<WeakNode>, mut node: encoding::Node) -> Self {
    let current = Self::new_raw(
      node
        .contents
        .pop()
        .map(|text| text.text)
        .unwrap_or_default(),
      node.icon,
      node.is_expanded,
      parent,
    );

    let children = node
      .children
      .into_iter()
      .map(|node| Self::from_encoding_rec(Some(current.downgrade()), node))
      .collect();

    current.inner.borrow_mut().children = children;
    current
  }

  fn get_node_by_line(&self, mut line: usize) -> (usize, Option<Self>) {
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
          .find(|node| node.inner.borrow().name == node_name)?
          .get_node_by_path(path)
      }
    }
  }

  /// Get the index of a [`Node`] in the node passed as argument, which must be its parent.
  ///
  /// If we don’t have any parent, returns `None`.
  fn get_index(&self, parent: &Node) -> Result<usize, NodeError> {
    for (i, child) in parent.inner.borrow().children.iter().enumerate() {
      if self == child {
        return Ok(i);
      }
    }

    Err(NodeError::NotContainedInParent)
  }

  #[cfg(test)]
  fn get_index_from_parent(&self) -> Result<usize, NodeError> {
    self.parent().and_then(|parent| self.get_index(&parent))
  }

  pub fn name(&self) -> String {
    self.inner.borrow().name.to_owned()
  }

  pub fn set_name(&self, name: impl Into<String>) {
    self.inner.borrow_mut().name = name.into();
  }

  pub fn icon(&self) -> String {
    self.inner.borrow().icon.to_owned()
  }

  pub fn set_icon(&self, icon: impl Into<String>) {
    self.inner.borrow_mut().icon = icon.into();
  }

  pub fn is_expanded(&self) -> bool {
    self.inner.borrow().is_expanded
  }

  pub fn set_expanded(&self, is_expanded: bool) {
    self.inner.borrow_mut().is_expanded = is_expanded;
  }

  pub fn parent(&self) -> Result<Node, NodeError> {
    self
      .inner
      .borrow()
      .parent
      .as_ref()
      .and_then(WeakNode::upgrade)
      .ok_or(NodeError::NoParent)
  }

  pub fn insert_top(&self, node: Node) {
    node.inner.borrow_mut().parent = Some(self.downgrade());
    let _ = self.inner.borrow_mut().children.insert(0, node);
  }

  pub fn insert_bottom(&self, node: Node) {
    node.inner.borrow_mut().parent = Some(self.downgrade());
    let _ = self.inner.borrow_mut().children.push(node);
  }

  pub fn insert_before(&self, node: Node) -> Result<(), NodeError> {
    let parent = self.parent()?;
    let i = self.get_index(&parent)?;

    node.inner.borrow_mut().parent = Some(parent.downgrade());
    let _ = parent.inner.borrow_mut().children.insert(i, node);
    Ok(())
  }

  pub fn insert_after(&self, node: Node) -> Result<(), NodeError> {
    let parent = self.parent()?;
    let i = self.get_index(&parent)? + 1;

    node.inner.borrow_mut().parent = Some(parent.downgrade());
    let _ = parent.inner.borrow_mut().children.insert(i, node);
    Ok(())
  }

  pub fn delete(&self, node: Node) -> Result<(), NodeError> {
    let mut inner = self.inner.borrow_mut();
    let i = inner
      .children
      .iter()
      .enumerate()
      .find_map(|(i, n)| if n == &node { Some(i) } else { None })
      .ok_or(NodeError::NotContainedInParent)?;

    let _ = inner.children.remove(i);
    Ok(())
  }

  pub fn toggle_expand(&self) {
    let mut node = self.inner.borrow_mut();
    node.is_expanded = !node.is_expanded;
  }
}

#[derive(Clone, Debug)]
pub struct NodeInner {
  name: String,
  icon: String,
  is_expanded: bool,
  parent: Option<WeakNode>,
  children: Vec<Node>,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum NodeError {
  #[error("cannot insert; no parent")]
  NoParent,

  #[error("the node is not contained in its supposed parent")]
  NotContainedInParent,
}

#[cfg(test)]
mod tests {
  use crate::{
    encoding::{self, TreeType, Version},
    node::{Node, Tree},
  };

  #[test]
  fn get_node_by_line_no_child() {
    let tree = Tree::from_encoding(encoding::Tree {
      version: Version::default(),
      ty: TreeType::Root,
      node: encoding::Node::new_by_expand_state("root", false, vec![]),
    });

    assert_eq!(
      tree
        .get_node_by_line(0)
        .as_ref()
        .map(|node| node.inner.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("root")
    );
    assert_eq!(tree.get_node_by_line(1), None);
    assert_eq!(tree.get_node_by_line(2), None);

    let tree = Tree::from_encoding(encoding::Tree {
      version: Version::default(),
      ty: TreeType::Root,
      node: encoding::Node::new_by_expand_state("root", true, vec![]),
    });

    assert_eq!(
      tree
        .get_node_by_line(0)
        .as_ref()
        .map(|node| node.inner.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("root")
    );
    assert_eq!(
      tree
        .get_node_by_line(0)
        .as_ref()
        .map(|node| node.inner.borrow())
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
    let tree = Tree::from_encoding(encoding::Tree {
      version: Version::default(),
      ty: TreeType::Root,
      node: encoding::Node::new_by_expand_state(
        "root",
        true,
        vec![
          encoding::Node::new_by_expand_state(
            "a",
            false,
            vec![
              encoding::Node::new_by_expand_state("x", false, vec![]),
              encoding::Node::new_by_expand_state("y", false, vec![]),
            ],
          ),
          encoding::Node::new_by_expand_state(
            "b",
            true,
            vec![encoding::Node::new_by_expand_state("z", false, vec![])],
          ),
          encoding::Node::new_by_expand_state("c", false, vec![]),
        ],
      ),
    });

    assert_eq!(
      tree
        .get_node_by_line(0)
        .as_ref()
        .map(|node| node.inner.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("root")
    );
    assert_eq!(
      tree
        .get_node_by_line(1)
        .as_ref()
        .map(|node| node.inner.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("a")
    );
    assert_eq!(
      tree
        .get_node_by_line(2)
        .as_ref()
        .map(|node| node.inner.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("b")
    );
    assert_eq!(
      tree
        .get_node_by_line(3)
        .as_ref()
        .map(|node| node.inner.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("z")
    );
    assert_eq!(
      tree
        .get_node_by_line(4)
        .as_ref()
        .map(|node| node.inner.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("c")
    );
  }

  #[test]
  fn get_node_by_path_no_child() {
    let tree = Tree::from_encoding(encoding::Tree {
      version: Version::default(),
      ty: TreeType::Root,
      node: encoding::Node::new_by_expand_state("root", false, vec![]),
    });

    assert_eq!(
      tree
        .get_node_by_line(0)
        .as_ref()
        .map(|node| node.inner.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("root")
    );
    assert_eq!(tree.get_node_by_path(["test"]), None);

    let tree = Tree::from_encoding(encoding::Tree {
      version: Version::default(),
      ty: TreeType::Root,
      node: encoding::Node::new_by_expand_state("root", true, vec![]),
    });

    assert_eq!(
      tree
        .get_node_by_line(0)
        .as_ref()
        .map(|node| node.inner.borrow())
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
    let tree = Tree::from_encoding(encoding::Tree {
      version: Version::default(),
      ty: TreeType::Root,
      node: encoding::Node::new_by_expand_state(
        "root",
        true,
        vec![
          encoding::Node::new_by_expand_state(
            "a",
            false,
            vec![
              encoding::Node::new_by_expand_state("x", false, vec![]),
              encoding::Node::new_by_expand_state("y", false, vec![]),
            ],
          ),
          encoding::Node::new_by_expand_state(
            "b",
            true,
            vec![encoding::Node::new_by_expand_state("z", false, vec![])],
          ),
          encoding::Node::new_by_expand_state("c", false, vec![]),
        ],
      ),
    });

    assert_eq!(
      tree
        .get_node_by_line(0)
        .as_ref()
        .map(|node| node.inner.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("root")
    );
    assert_eq!(
      tree
        .get_node_by_path(["a"])
        .as_ref()
        .map(|node| node.inner.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("a")
    );
    assert_eq!(
      tree
        .get_node_by_path(["a", "x"])
        .as_ref()
        .map(|node| node.inner.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("x")
    );
    assert_eq!(
      tree
        .get_node_by_path(["a", "y"])
        .as_ref()
        .map(|node| node.inner.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("y")
    );
    assert_eq!(
      tree
        .get_node_by_path(["b"])
        .as_ref()
        .map(|node| node.inner.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("b")
    );
    assert_eq!(
      tree
        .get_node_by_path(["b", "z"])
        .as_ref()
        .map(|node| node.inner.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("z")
    );
    assert_eq!(
      tree
        .get_node_by_path(["c"])
        .as_ref()
        .map(|node| node.inner.borrow())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("c")
    );
  }

  #[test]
  fn get_index_from_parent() {
    let tree = Tree::from_encoding(encoding::Tree {
      version: Version::default(),
      ty: TreeType::Root,
      node: encoding::Node::new_by_expand_state(
        "root",
        true,
        vec![
          encoding::Node::new_by_expand_state(
            "a",
            false,
            vec![
              encoding::Node::new_by_expand_state("x", false, vec![]),
              encoding::Node::new_by_expand_state("y", false, vec![]),
            ],
          ),
          encoding::Node::new_by_expand_state(
            "b",
            true,
            vec![encoding::Node::new_by_expand_state("z", false, vec![])],
          ),
          encoding::Node::new_by_expand_state("c", false, vec![]),
        ],
      ),
    });

    assert_eq!(
      tree
        .get_node_by_path(["a", "x"])
        .unwrap()
        .get_index_from_parent(),
      Ok(0)
    );
    assert_eq!(
      tree
        .get_node_by_path(["a", "y"])
        .unwrap()
        .get_index_from_parent(),
      Ok(1)
    );
  }

  #[test]
  fn insert() {
    let tree = Tree::new("root", "");
    let node = tree.get_node_by_line(0).unwrap();

    node.insert_bottom(Node::new("x", ""));
    node.insert_bottom(Node::new("y", ""));
    node.insert_bottom(Node::new("z", ""));
    node.insert_top(Node::new("c", ""));
    node.insert_top(Node::new("b", ""));
    node.insert_top(Node::new("a", ""));

    tree
      .get_node_by_path(["c"])
      .unwrap()
      .insert_after(Node::new("d", ""))
      .unwrap();

    tree
      .get_node_by_path(["x"])
      .unwrap()
      .insert_before(Node::new("w", ""))
      .unwrap();

    assert_eq!(
      tree
        .get_node_by_path(["a"])
        .unwrap()
        .get_index_from_parent(),
      Ok(0)
    );
    assert_eq!(
      tree
        .get_node_by_path(["b"])
        .unwrap()
        .get_index_from_parent(),
      Ok(1)
    );
    assert_eq!(
      tree
        .get_node_by_path(["c"])
        .unwrap()
        .get_index_from_parent(),
      Ok(2)
    );
    assert_eq!(
      tree
        .get_node_by_path(["d"])
        .unwrap()
        .get_index_from_parent(),
      Ok(3)
    );
    assert_eq!(
      tree
        .get_node_by_path(["w"])
        .unwrap()
        .get_index_from_parent(),
      Ok(4)
    );
    assert_eq!(
      tree
        .get_node_by_path(["x"])
        .unwrap()
        .get_index_from_parent(),
      Ok(5)
    );
    assert_eq!(
      tree
        .get_node_by_path(["y"])
        .unwrap()
        .get_index_from_parent(),
      Ok(6)
    );
    assert_eq!(
      tree
        .get_node_by_path(["z"])
        .unwrap()
        .get_index_from_parent(),
      Ok(7)
    );
  }

  #[test]
  fn delete() {
    let tree = Tree::new("root", "");
    let node = tree.get_node_by_line(0).unwrap();

    node.insert_bottom(Node::new("x", ""));
    node.insert_bottom(Node::new("y", ""));

    let x = tree.get_node_by_path(["x"]).unwrap();
    x.insert_bottom(Node::new("a", ""));
    x.insert_bottom(Node::new("b", ""));
    x.insert_bottom(Node::new("c", ""));

    let b = tree.get_node_by_path(["x", "b"]).unwrap();
    x.delete(b).unwrap();

    assert_eq!(tree.get_node_by_path(["x", "b"]), None);
  }
}
