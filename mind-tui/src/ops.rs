/// Insert mode for insertion and move operations.
#[derive(Clone, Copy, Debug, Default)]
pub enum InsertMode {
  /// Insert the node inside the selected node, at the top.
  InsideTop,

  /// Insert the node inside the selected node, at the bottom.
  #[default]
  InsideBottom,

  /// Insert the node as a sibling, just before the selected node (if the selected has a parent).
  Before,

  /// Insert the node as a sibling, just after the selected node (if the selected has a parent)
  After,
}
