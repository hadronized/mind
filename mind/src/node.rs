//! Node operations

use crate::encoding::{self, TreeType};
use serde::{Deserialize, Serialize};
use std::{
  io::{self, Write},
  path::PathBuf,
  sync::{Arc, RwLock, RwLockReadGuard, Weak},
};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(from = "encoding::Tree", into = "encoding::Tree")]
pub struct Tree {
  version: encoding::Version,
  ty: TreeType,
  node: Node,
}

impl From<encoding::Tree> for Tree {
  fn from(value: encoding::Tree) -> Self {
    Self::from_encoding(value)
  }
}

impl From<Tree> for encoding::Tree {
  fn from(value: Tree) -> Self {
    value.into_encoding()
  }
}

impl Tree {
  pub fn new(name: impl AsRef<str>, icon: impl AsRef<str>) -> Self {
    Self {
      version: encoding::Version::current(),
      ty: TreeType::Root,
      node: Node::new(name, icon),
    }
  }

  pub fn from_encoding(tree: encoding::Tree) -> Self {
    Self {
      version: tree.version,
      ty: tree.ty,
      node: Node::from_encoding(tree.node),
    }
  }

  pub fn into_encoding(&self) -> encoding::Tree {
    encoding::Tree {
      version: self.version,
      ty: self.ty,
      node: self.node.into_encoding(),
    }
  }

  /// Get the root node.
  pub fn root(&self) -> Node {
    self.node.clone()
  }

  /// Get a [`Node`] by line number.
  ///
  /// 0-indexed.
  pub fn get_node_by_line(&self, line: usize) -> Option<Node> {
    let (_, node) = self.node.get_node_by_line(line);
    node
  }

  /// Get a [`Node`] by path, e.g. `/root/a/b/c/d`.
  pub fn get_node_by_path<'a>(
    &self,
    path: impl IntoIterator<Item = &'a str>,
    auto_create_nodes: bool,
  ) -> Option<Node> {
    self
      .node
      .get_node_by_path(path.into_iter(), auto_create_nodes)
  }
}

/// Weak version of [`Node`], mainly for parent nodes.
///
/// Not supposed to be used as-is; convert to [`Node`] when needed.
#[derive(Clone, Debug)]
pub struct WeakNode {
  inner: Weak<RwLock<NodeInner>>,
}

impl WeakNode {
  fn upgrade(&self) -> Option<Node> {
    self.inner.upgrade().map(|inner| Node { inner })
  }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(from = "encoding::Node", into = "encoding::Node")]
pub struct Node {
  inner: Arc<RwLock<NodeInner>>,
}

impl Eq for Node {}

impl PartialEq for Node {
  fn eq(&self, other: &Self) -> bool {
    Arc::as_ptr(&self.inner).eq(&Arc::as_ptr(&other.inner))
  }
}

impl From<encoding::Node> for Node {
  fn from(value: encoding::Node) -> Self {
    Self::from_encoding(value)
  }
}

impl From<Node> for encoding::Node {
  fn from(value: Node) -> Self {
    value.into_encoding()
  }
}

impl Node {
  pub fn new(name: impl AsRef<str>, icon: impl AsRef<str>) -> Self {
    Self::new_raw(name.as_ref(), icon.as_ref(), false, None)
  }

  fn new_raw(name: &str, icon: &str, is_expanded: bool, parent: Option<WeakNode>) -> Self {
    let name = name.trim().to_owned();

    // we only trim left because sometimes, the right space is meaningful for some icons
    let icon = icon.trim_start().to_owned();

    Self {
      inner: Arc::new(RwLock::new(NodeInner {
        name,
        icon,
        is_expanded,
        parent,
        prev: None,
        next: None,
        data: None,
        children: Vec::new(),
      })),
    }
  }

  fn downgrade(&self) -> WeakNode {
    WeakNode {
      inner: Arc::downgrade(&self.inner),
    }
  }

  pub fn from_encoding(node: encoding::Node) -> Self {
    Self::from_encoding_rec(None, node)
  }

  fn from_encoding_rec(parent: Option<WeakNode>, mut node: encoding::Node) -> Self {
    let current = Self::new_raw(
      &node
        .contents
        .pop()
        .map(|text| text.text)
        .unwrap_or_default(),
      &node.icon,
      node.is_expanded,
      parent,
    );

    let children: Vec<_> = node
      .children
      .into_iter()
      .map(|node| Self::from_encoding_rec(Some(current.downgrade()), node))
      .collect();

    // set prev / next in children
    for (a, b) in children.iter().zip(children.iter().skip(1)) {
      a.inner.write().unwrap().next = Some(b.clone());
      b.inner.write().unwrap().prev = Some(a.clone());
    }

    let data = node
      .data
      .map(NodeData::file)
      .or_else(move || node.url.map(NodeData::link));

    {
      let mut inner = current.inner.write().unwrap();
      inner.children = children;
      inner.data = data;
    }

    current
  }

  pub fn into_encoding(&self) -> encoding::Node {
    let node = self.inner.read().unwrap();
    let data;
    let url;

    match node.data {
      Some(NodeData::File(ref path)) => {
        data = Some(path.clone());
        url = None;
      }

      Some(NodeData::Link(ref link)) => {
        url = Some(link.clone());
        data = None;
      }

      None => {
        data = None;
        url = None;
      }
    }

    encoding::Node {
      icon: node.icon.clone(),
      is_expanded: node.is_expanded,
      contents: vec![encoding::Text {
        text: node.name.clone(),
      }],
      data,
      url,
      children: node.children.iter().map(Self::into_encoding).collect(),
    }
  }

  fn get_node_by_line(&self, mut line: usize) -> (usize, Option<Self>) {
    let node = self.inner.read().unwrap();

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

  fn get_node_by_path<'a>(
    &self,
    mut path: impl Iterator<Item = &'a str>,
    auto_create_nodes: bool,
  ) -> Option<Self> {
    let node = self.inner.read().unwrap();

    match path.next() {
      None => Some(self.clone()),

      Some(node_name) => {
        // find the node in the children list, and if it doesn’t exist, it means the node we are looking for doesn’t exist;
        // abort early if we don’t need to create the node
        match node
          .children
          .iter()
          .find(|node| node.inner.read().unwrap().name == node_name)
        {
          Some(child) => child.get_node_by_path(path, auto_create_nodes),
          None => {
            drop(node);

            if auto_create_nodes {
              let child = Node::new(node_name, "");
              self.insert_bottom(child.clone());
              child.get_node_by_path(path, auto_create_nodes)
            } else {
              None
            }
          }
        }
      }
    }
  }

  /// Get the index of a [`Node`] in the node passed as argument, which must be its parent.
  ///
  /// If we don’t have any parent, returns `None`.
  fn get_index(&self, parent: &Node) -> Result<usize, NodeError> {
    for (i, child) in parent.inner.read().unwrap().children.iter().enumerate() {
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

  pub fn children(&self) -> Children {
    Children {
      borrow: self.inner.read().unwrap(),
    }
  }

  pub fn has_children(&self) -> bool {
    !self.inner.read().unwrap().children.is_empty()
  }

  pub fn name(&self) -> String {
    self.inner.read().unwrap().name.to_owned()
  }

  pub fn set_name(&self, name: impl AsRef<str>) -> Result<(), NodeError> {
    let name = name.as_ref().trim().to_owned();

    if name.is_empty() {
      return Err(NodeError::EmptyName);
    }

    self.inner.write().unwrap().name = name;
    Ok(())
  }

  pub fn icon(&self) -> String {
    let icon = &self.inner.read().unwrap().icon;

    if icon.is_empty() {
      String::new()
    } else {
      format!("{} ", icon)
    }
  }

  pub fn set_icon(&self, icon: impl AsRef<str>) {
    let icon = icon.as_ref().trim_start().to_owned();
    self.inner.write().unwrap().icon = icon;
  }

  pub fn data(&self) -> Option<NodeData> {
    self.inner.read().unwrap().data.clone()
  }

  pub fn set_data(&self, data: NodeData) -> Result<(), NodeError> {
    let current = self.inner.read().unwrap();

    match (current.data.as_ref(), &data) {
      // if nothing is set, set it
      (None, NodeData::Link(link)) => {
        if link.is_empty() {
          return Err(NodeError::NoData);
        }

        drop(current);
        self.inner.write().unwrap().data = Some(data);
      }

      (None, NodeData::File(path)) => {
        if path.as_os_str().is_empty() {
          return Err(NodeError::NoData);
        }

        drop(current);
        self.inner.write().unwrap().data = Some(data);
      }

      (Some(NodeData::Link(_)), NodeData::Link(_)) => {
        drop(current);
        self.inner.write().unwrap().data = Some(data)
      }

      (Some(NodeData::File(_)), NodeData::File(_)) => return Err(NodeError::FileDataAlreadyExists),

      // otherwise it’s a data type mismatch
      _ => return Err(NodeError::MismatchDataType),
    }

    Ok(())
  }

  pub fn is_expanded(&self) -> bool {
    self.inner.read().unwrap().is_expanded
  }

  pub fn set_expanded(&self, is_expanded: bool) {
    self.inner.write().unwrap().is_expanded = is_expanded;
  }

  pub fn parent(&self) -> Result<Node, NodeError> {
    self
      .inner
      .read()
      .unwrap()
      .parent
      .as_ref()
      .and_then(WeakNode::upgrade)
      .ok_or(NodeError::NoParent)
  }

  pub fn prev(&self) -> Option<Node> {
    self.inner.read().unwrap().prev.clone()
  }

  pub fn next(&self) -> Option<Node> {
    self.inner.read().unwrap().next.clone()
  }

  pub fn insert_top(&self, node: Node) {
    node.inner.write().unwrap().parent = Some(self.downgrade());

    let mut children = self.inner.write().unwrap();
    if let Some(first) = children.children.first() {
      first.inner.write().unwrap().prev = Some(node.clone());
      node.inner.write().unwrap().next = Some(first.clone());
    }

    children.children.insert(0, node);
  }

  pub fn insert_bottom(&self, node: Node) {
    node.inner.write().unwrap().parent = Some(self.downgrade());

    let mut children = self.inner.write().unwrap();
    if let Some(last) = children.children.last() {
      last.inner.write().unwrap().next = Some(node.clone());
      node.inner.write().unwrap().prev = Some(last.clone());
    }

    children.children.push(node);
  }

  pub fn insert_before(&self, node: Node) -> Result<(), NodeError> {
    let parent = self.parent()?;

    // set parent link, prev / next between selected node and node to insert
    {
      let mut inner = node.inner.write().unwrap();
      inner.parent = Some(parent.downgrade());
      inner.next = Some(self.clone());
      self.inner.write().unwrap().prev = Some(node.clone());
    }

    let i = self.get_index(&parent)?;
    let mut children = parent.inner.write().unwrap();

    // if there is a node in the children before the one we add, we set its next and set our prev
    if i > 0 {
      if let Some(prev) = children.children.get(i - 1) {
        prev.inner.write().unwrap().next = Some(node.clone());
        node.inner.write().unwrap().prev = Some(prev.clone());
      }
    }

    children.children.insert(i, node);

    Ok(())
  }

  pub fn insert_after(&self, node: Node) -> Result<(), NodeError> {
    let parent = self.parent()?;

    // set parent link, prev / next between selected node and node to insert
    {
      let mut inner = node.inner.write().unwrap();
      inner.parent = Some(parent.downgrade());
      inner.prev = Some(self.clone());
      self.inner.write().unwrap().next = Some(node.clone());
    }

    let i = self.get_index(&parent)? + 1;
    let mut children = parent.inner.write().unwrap();

    // if there is a node in the children before the one we add, we set its next and set our prev
    if let Some(next) = children.children.get(i) {
      next.inner.write().unwrap().prev = Some(node.clone());
      node.inner.write().unwrap().next = Some(next.clone());
    }

    children.children.insert(i, node);
    Ok(())
  }

  pub fn delete(&self, node: Node) -> Result<(), NodeError> {
    // find the child node in order to delete it by index
    let mut inner = self.inner.write().unwrap();
    let i = inner
      .children
      .iter()
      .enumerate()
      .find_map(|(i, n)| if n == &node { Some(i) } else { None })
      .ok_or(NodeError::NotContainedInParent)?;

    let _ = inner.children.remove(i);

    // removing sibling; if we have a previous, connect to next; if we have a next, connect to prev
    let prev = node.prev();
    let next = node.next();

    if let Some(prev) = prev.as_ref() {
      prev.inner.write().unwrap().next = next.clone();
    }

    if let Some(next) = next {
      next.inner.write().unwrap().prev = prev;
    }
    Ok(())
  }

  pub fn move_top(&self, node: Node) -> Result<(), NodeError> {
    let parent = node.parent()?;

    parent.delete(node.clone())?;
    self.insert_top(node);
    Ok(())
  }

  pub fn move_bottom(&self, node: Node) -> Result<(), NodeError> {
    let parent = node.parent()?;

    parent.delete(node.clone())?;
    self.insert_bottom(node);
    Ok(())
  }

  pub fn move_before(&self, node: Node) -> Result<(), NodeError> {
    let parent = node.parent()?;

    parent.delete(node.clone())?;
    self.insert_before(node)?;
    Ok(())
  }

  pub fn move_after(&self, node: Node) -> Result<(), NodeError> {
    let parent = node.parent()?;

    parent.delete(node.clone())?;
    self.insert_after(node)?;
    Ok(())
  }

  pub fn toggle_expand(&self) {
    let mut node = self.inner.write().unwrap();
    node.is_expanded = !node.is_expanded;
  }

  pub fn paths(&self, prefix: impl AsRef<str>, filter: NodeFilter) -> Vec<String> {
    let prefix = prefix.as_ref();
    let mut all_paths = Vec::new();

    if filter.accepts(self) {
      all_paths.push(prefix.to_owned());
    }

    let prefix = if prefix == "/" { "" } else { prefix };
    self.paths_rec(prefix, &mut all_paths, filter);
    all_paths
  }

  fn paths_rec(&self, parent: &str, paths: &mut Vec<String>, filter: NodeFilter) {
    for child in &self.inner.read().unwrap().children {
      let path = format!("{parent}/{name}", name = child.name());

      if filter.accepts(child) {
        paths.push(path.clone());
      }

      child.paths_rec(&path, paths, filter);
    }
  }

  /// Write paths to the provided writer.
  pub fn write_paths(
    &self,
    prefix: &str,
    filter: NodeFilter,
    writer: &mut impl Write,
  ) -> Result<(), NodeError> {
    for path in self.paths(prefix, filter) {
      writeln!(writer, "{}", path).map_err(NodeError::CannotWritePaths)?;
    }

    Ok(())
  }
}

#[derive(Clone, Debug)]
pub struct NodeInner {
  name: String,
  icon: String,
  is_expanded: bool,
  parent: Option<WeakNode>,
  prev: Option<Node>,
  next: Option<Node>,
  data: Option<NodeData>,
  children: Vec<Node>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NodeData {
  File(PathBuf),
  Link(String),
}

impl NodeData {
  pub fn file(path: impl Into<PathBuf>) -> Self {
    NodeData::File(path.into())
  }

  pub fn link(link: impl Into<String>) -> Self {
    NodeData::Link(link.into())
  }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum NodeFilter {
  #[default]
  Always,
  FileOrLink,
  FileOnly,
  LinkOnly,
}

impl NodeFilter {
  pub fn new(file: bool, uri: bool) -> Self {
    match (file, uri) {
      (true, true) => Self::FileOrLink,
      (true, false) => Self::FileOnly,
      (false, true) => Self::LinkOnly,
      (false, false) => Self::Always,
    }
  }

  fn accepts(&self, node: &Node) -> bool {
    match self {
      NodeFilter::Always => true,
      NodeFilter::FileOrLink => node.inner.read().unwrap().data.is_some(),
      NodeFilter::FileOnly => matches!(node.inner.read().unwrap().data, Some(NodeData::File(..))),
      NodeFilter::LinkOnly => matches!(node.inner.read().unwrap().data, Some(NodeData::Link(..))),
    }
  }
}

/// An iterator on a node children.
#[derive(Debug)]
pub struct Children<'a> {
  borrow: RwLockReadGuard<'a, NodeInner>,
}

impl<'a> Children<'a> {
  pub fn into_iter(&'a self) -> impl Iterator<Item = &'_ Node> {
    self.borrow.children.iter()
  }

  pub fn all_except_last(&'a self) -> impl Iterator<Item = &'_ Node> {
    self.borrow.children[..self.borrow.children.len() - 1].iter()
  }

  pub fn last(&'a self) -> Option<&'a Node> {
    self.borrow.children.last()
  }
}

/// A node cursor to ease moving around.
#[derive(Clone, Debug)]
pub struct Cursor {
  node: Node,
}

impl Cursor {
  pub fn new(node: Node) -> Self {
    Self { node }
  }

  /// Get access to the underlying [`Node`].
  pub fn node(&self) -> &Node {
    &self.node
  }

  /// Check whether the node is expanded.
  pub fn is_expanded(&self) -> bool {
    self.node.inner.read().unwrap().is_expanded
  }

  /// Check whether the cursor points to the given node.
  pub fn points_to(&self, node: &Node) -> bool {
    &self.node == node
  }

  pub fn toggle_expand(&self) {
    let mut inner = self.node.inner.write().unwrap();
    inner.is_expanded = !inner.is_expanded;
  }

  /// Go to parent.
  ///
  /// Return `false` if it has no parent.
  pub fn parent(&mut self) -> bool {
    let parent = self.node.inner.read().unwrap().parent.clone();
    if let Some(parent) = parent.and_then(|n| n.upgrade()) {
      self.node = parent;
      true
    } else {
      false
    }
  }

  /// Go to previous sibling.
  ///
  /// Return `false` if it has no previous sibling.
  pub fn prev_sibling(&mut self) -> bool {
    let prev = self.node.inner.read().unwrap().prev.clone();
    if let Some(prev) = prev {
      self.node = prev;
      true
    } else {
      false
    }
  }

  /// Go to next sibling.
  ///
  /// Return `false` if it has no nextious sibling.
  pub fn next_sibling(&mut self) -> bool {
    let next = self.node.inner.read().unwrap().next.clone();
    if let Some(next) = next {
      self.node = next;
      true
    } else {
      false
    }
  }

  /// Go to the first child, if any.
  ///
  /// Return `false` if it has no child.
  pub fn first_child(&mut self) -> bool {
    let child = self.node.inner.read().unwrap().children.first().cloned();
    if let Some(child) = child {
      self.node = child;
      true
    } else {
      false
    }
  }

  /// Go to the “previous” node.
  ///
  /// The previous node is the visually preceding node. That function will respect expanded / collapsed nodes.
  pub fn visual_prev(&mut self) -> bool {
    if self.prev_sibling() {
      // ensure we go down the previous node
      while self.is_expanded() && self.first_child() {
        while self.next_sibling() {}
      }

      true
    } else {
      self.parent()
    }
  }

  /// Go to the “next” node.
  ///
  /// The next node is the visually succeeding node. That function will respect expanded / collapsed nodes.
  pub fn visual_next(&mut self) -> bool {
    if self.is_expanded() && self.first_child() || self.next_sibling() {
      true
    } else {
      let mut cursor = self.clone();
      while cursor.parent() {
        if cursor.next_sibling() {
          *self = cursor;
          return true;
        }
      }

      false
    }
  }
}

#[derive(Debug, Error)]
pub enum NodeError {
  #[error("cannot insert; no parent")]
  NoParent,

  #[error("the node is not contained in its supposed parent")]
  NotContainedInParent,

  #[error("cannot set name; name cannot be empty")]
  EmptyName,

  #[error("cannot set data; file data already exists")]
  FileDataAlreadyExists,

  #[error("cannot set data; already exists with a different type")]
  MismatchDataType,

  #[error("no data")]
  NoData,

  #[error("cannot create associated data file: {0}")]
  CannotCreateDataFile(io::Error),

  #[error("cannot write paths")]
  CannotWritePaths(io::Error),
}

/// Split a string in the form of `/NodeA/NodeB/…` into an iterator of path segment.
///
/// Use that function to pass to various tree and node API functions expecting a path.
pub fn path_iter(path: &str) -> impl Iterator<Item = &str> {
  path.split('/').filter(|frag| !frag.trim().is_empty())
}

#[cfg(test)]
mod tests {
  use crate::{
    encoding::{self, TreeType, Version},
    node::{Node, NodeData, NodeError, NodeFilter, Tree},
  };

  use super::path_iter;

  #[test]
  fn path_iter_test() {
    assert_eq!(path_iter("").collect::<Vec<_>>(), Vec::<&str>::new());
    assert_eq!(path_iter("/").collect::<Vec<_>>(), Vec::<&str>::new());
    assert_eq!(path_iter("//").collect::<Vec<_>>(), Vec::<&str>::new());
    assert_eq!(
      path_iter("/Foo/Bar/Zoo").collect::<Vec<_>>(),
      vec!["Foo", "Bar", "Zoo"]
    );

    assert_eq!(path_iter("/A B").collect::<Vec<_>>(), vec!["A B"]);
  }

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
        .map(|node| node.inner.read().unwrap())
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
        .map(|node| node.inner.read().unwrap())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("root")
    );
    assert_eq!(
      tree
        .get_node_by_line(0)
        .as_ref()
        .map(|node| node.inner.read().unwrap())
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
        .map(|node| node.inner.read().unwrap())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("root")
    );
    assert_eq!(
      tree
        .get_node_by_line(1)
        .as_ref()
        .map(|node| node.inner.read().unwrap())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("a")
    );
    assert_eq!(
      tree
        .get_node_by_line(2)
        .as_ref()
        .map(|node| node.inner.read().unwrap())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("b")
    );
    assert_eq!(
      tree
        .get_node_by_line(3)
        .as_ref()
        .map(|node| node.inner.read().unwrap())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("z")
    );
    assert_eq!(
      tree
        .get_node_by_line(4)
        .as_ref()
        .map(|node| node.inner.read().unwrap())
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
        .map(|node| node.inner.read().unwrap())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("root")
    );
    assert_eq!(tree.get_node_by_path(["test"], false), None);

    let tree = Tree::from_encoding(encoding::Tree {
      version: Version::default(),
      ty: TreeType::Root,
      node: encoding::Node::new_by_expand_state("root", true, vec![]),
    });

    assert_eq!(
      tree
        .get_node_by_line(0)
        .as_ref()
        .map(|node| node.inner.read().unwrap())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("root")
    );
    assert_eq!(tree.get_node_by_path(["test"], false), None);
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
        .map(|node| node.inner.read().unwrap())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("root")
    );
    assert_eq!(
      tree
        .get_node_by_path(["a"], false)
        .as_ref()
        .map(|node| node.inner.read().unwrap())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("a")
    );
    assert_eq!(
      tree
        .get_node_by_path(["a", "x"], false)
        .as_ref()
        .map(|node| node.inner.read().unwrap())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("x")
    );
    assert_eq!(
      tree
        .get_node_by_path(["a", "y"], false)
        .as_ref()
        .map(|node| node.inner.read().unwrap())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("y")
    );
    assert_eq!(
      tree
        .get_node_by_path(["b"], false)
        .as_ref()
        .map(|node| node.inner.read().unwrap())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("b")
    );
    assert_eq!(
      tree
        .get_node_by_path(["b", "z"], false)
        .as_ref()
        .map(|node| node.inner.read().unwrap())
        .as_ref()
        .map(|node| node.name.as_str()),
      Some("z")
    );
    assert_eq!(
      tree
        .get_node_by_path(["c"], false)
        .as_ref()
        .map(|node| node.inner.read().unwrap())
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

    assert!(matches!(
      tree
        .get_node_by_path(["a", "x"], false)
        .unwrap()
        .get_index_from_parent(),
      Ok(0)
    ));
    assert!(matches!(
      tree
        .get_node_by_path(["a", "y"], false)
        .unwrap()
        .get_index_from_parent(),
      Ok(1)
    ));
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
      .get_node_by_path(["c"], false)
      .unwrap()
      .insert_after(Node::new("d", ""))
      .unwrap();

    tree
      .get_node_by_path(["x"], false)
      .unwrap()
      .insert_before(Node::new("w", ""))
      .unwrap();

    assert!(matches!(
      tree
        .get_node_by_path(["a"], false)
        .unwrap()
        .get_index_from_parent(),
      Ok(0)
    ));
    assert!(matches!(
      tree
        .get_node_by_path(["b"], false)
        .unwrap()
        .get_index_from_parent(),
      Ok(1)
    ));
    assert!(matches!(
      tree
        .get_node_by_path(["c"], false)
        .unwrap()
        .get_index_from_parent(),
      Ok(2)
    ));
    assert!(matches!(
      tree
        .get_node_by_path(["d"], false)
        .unwrap()
        .get_index_from_parent(),
      Ok(3)
    ));
    assert!(matches!(
      tree
        .get_node_by_path(["w"], false)
        .unwrap()
        .get_index_from_parent(),
      Ok(4)
    ));
    assert!(matches!(
      tree
        .get_node_by_path(["x"], false)
        .unwrap()
        .get_index_from_parent(),
      Ok(5)
    ));
    assert!(matches!(
      tree
        .get_node_by_path(["y"], false)
        .unwrap()
        .get_index_from_parent(),
      Ok(6)
    ));
    assert!(matches!(
      tree
        .get_node_by_path(["z"], false)
        .unwrap()
        .get_index_from_parent(),
      Ok(7)
    ));
  }

  #[test]
  fn delete() {
    let tree = Tree::new("root", "");
    let node = tree.get_node_by_line(0).unwrap();

    node.insert_bottom(Node::new("x", ""));
    node.insert_bottom(Node::new("y", ""));

    let x = tree.get_node_by_path(["x"], false).unwrap();
    x.insert_bottom(Node::new("a", ""));
    x.insert_bottom(Node::new("b", ""));
    x.insert_bottom(Node::new("c", ""));

    let b = tree.get_node_by_path(["x", "b"], false).unwrap();
    x.delete(b).unwrap();

    assert_eq!(tree.get_node_by_path(["x", "b"], false), None);
  }

  #[test]
  fn select_move() {
    let tree = Tree::new("root", "");
    let node = tree.get_node_by_line(0).unwrap();

    node.insert_bottom(Node::new("x", ""));
    node.insert_bottom(Node::new("y", ""));
    node.insert_bottom(Node::new("z", ""));
    node.insert_top(Node::new("c", ""));
    node.insert_top(Node::new("b", ""));
    node.insert_top(Node::new("a", ""));

    let a = tree.get_node_by_path(["a"], false).unwrap();
    let b = tree.get_node_by_path(["b"], false).unwrap();
    let c = tree.get_node_by_path(["c"], false).unwrap();
    let x = tree.get_node_by_path(["x"], false).unwrap();
    let y = tree.get_node_by_path(["y"], false).unwrap();
    let z = tree.get_node_by_path(["z"], false).unwrap();

    a.move_bottom(x.clone()).unwrap();
    a.move_top(y).unwrap();
    x.move_after(z.clone()).unwrap();
    z.move_before(b).unwrap();
    node.move_bottom(c).unwrap();

    assert!(matches!(
      tree
        .get_node_by_path(["a", "y"], false)
        .unwrap()
        .get_index_from_parent(),
      Ok(0)
    ));
    assert!(matches!(
      tree
        .get_node_by_path(["a", "x"], false)
        .unwrap()
        .get_index_from_parent(),
      Ok(1)
    ));
    assert!(matches!(
      tree
        .get_node_by_path(["a", "b"], false)
        .unwrap()
        .get_index_from_parent(),
      Ok(2)
    ));
    assert!(matches!(
      tree
        .get_node_by_path(["a", "z"], false)
        .unwrap()
        .get_index_from_parent(),
      Ok(3)
    ));
    assert!(matches!(
      tree
        .get_node_by_path(["c"], false)
        .unwrap()
        .get_index_from_parent(),
      Ok(1)
    ));
  }

  #[test]
  fn test_paths() {
    let tree = Tree::new("root", "");
    let node = tree.get_node_by_line(0).unwrap();

    node.insert_bottom(Node::new("x", ""));
    node.insert_bottom(Node::new("y", ""));

    let x = tree.get_node_by_path(["x"], false).unwrap();
    x.insert_bottom(Node::new("a", ""));
    x.insert_bottom(Node::new("b", ""));
    x.insert_bottom(Node::new("c", ""));

    assert_eq!(
      node.paths("", NodeFilter::Always),
      vec!["", "/x", "/x/a", "/x/b", "/x/c", "/y"],
    );
  }

  #[test]
  fn data() {
    let node = Node::new("test", "");

    assert_eq!(node.data(), None);

    assert!(matches!(
      node.set_data(NodeData::file("/tmp/foo.md")),
      Ok(())
    ));
    assert_eq!(node.data(), Some(NodeData::file("/tmp/foo.md")));

    assert!(matches!(
      node.set_data(NodeData::file("/tmp/bar.rs")),
      Err(NodeError::FileDataAlreadyExists)
    ));
    assert_eq!(node.data(), Some(NodeData::file("/tmp/foo.md")));

    assert!(matches!(
      node.set_data(NodeData::link("https://foo.bar")),
      Err(NodeError::MismatchDataType)
    ));
  }
}
