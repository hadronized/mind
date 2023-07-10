use std::sync::mpsc::Sender;

use crossterm::event::{KeyCode, KeyEvent};
use mind_tree::node::{Cursor, Node};
use tui::{
  buffer::Buffer,
  layout::Rect,
  style::{Color, Modifier, Style},
  text::Span,
  widgets::Widget,
};

use crate::{
  error::AppError,
  event::{Event, HandledEvent, RawEventHandler},
  ops::InsertMode,
};

use super::user_input::UserInputPrompt;

/// TUI version of a tree.
#[derive(Debug)]
pub struct TuiTree {
  rect: Rect,

  event_sx: Sender<Event>,

  /// Root node.
  root: Node,

  /// Currently selected node ID.
  selected_node_id: usize,

  /// Node cursor.
  cursor: Cursor,

  /// Top-level shift.
  top_shift: u16,

  /// Prompt used to ask stuff from the user.
  input_prompt: UserInputPrompt,

  /// Event to be emitted once the input prompt is entered.
  ///
  /// Can be cancelled if the prompt is aborted.
  input_pending_event: Option<Event>,
}

impl TuiTree {
  pub fn new(rect: Rect, event_sx: Sender<Event>, root: Node) -> Self {
    let node_cursor = Cursor::new(root.clone());

    Self {
      rect,
      event_sx,
      root,
      selected_node_id: 0,
      cursor: node_cursor,
      top_shift: 0,
      input_prompt: UserInputPrompt::default(),
      input_pending_event: None,
    }
  }

  /// Get the area the tree will render to.
  pub fn area(&self) -> &Rect {
    &self.rect
  }

  /// Set the area the tree will be rendered to.
  pub fn set_area(&mut self, rect: Rect) {
    self.rect = rect;
  }

  fn emit_event(&self, event: Event) -> Result<(), AppError> {
    log::debug!("emitting event: {event:?}");

    self
      .event_sx
      .send(event)
      .map_err(|e| AppError::Event(e.to_string()))?;
    Ok(())
  }

  pub fn select_prev_node(&mut self) -> bool {
    if self.cursor.visual_prev() {
      self.selected_node_id -= 1;
      self.adjust_view();
      true
    } else {
      false
    }
  }

  pub fn select_next_node(&mut self) -> bool {
    if self.cursor.visual_next() {
      self.selected_node_id += 1;
      self.adjust_view();
      true
    } else {
      false
    }
  }

  pub fn shift_selected_node_id(&mut self, shift: isize) {
    self.selected_node_id = (self.selected_node_id as isize + shift) as usize;
  }

  fn adjust_view(&mut self) {
    let y = self.selected_node_id as isize - self.top_shift as isize;

    if y < 0 {
      self.top_shift -= -y as u16;
    } else if y >= self.rect.height as isize {
      self.top_shift += 1 + y as u16 - self.rect.height;
    }
  }

  fn open_prompt_insert_node(&mut self, title: &str, mode: InsertMode) {
    self.input_prompt.show_with_title(title);
    self.input_pending_event = Some(Event::InsertNode {
      id: self.selected_node_id,
      mode,
      name: String::new(),
    });
  }

  fn open_prompt_delete_node(&mut self) {
    self.input_prompt.show_with_title("delete node (y/N):");
    self.input_pending_event = Some(Event::DeleteNode {
      id: self.selected_node_id,
    });
  }
}

impl<'a> Widget for &'a TuiTree {
  fn render(self, area: Rect, buf: &mut Buffer) {
    render_with_indent(
      &self.root,
      self.top_shift,
      0,
      area,
      buf,
      &Indent::default(),
      false,
      &self.cursor,
    );

    if let Some(prompt) = self.input_prompt.prompt() {
      prompt.render(
        Rect {
          y: area.height - 1,
          height: 1,
          ..area
        },
        buf,
      );

      // TODO: position the cursor
    }
  }
}

impl RawEventHandler for TuiTree {
  type Feedback = ();

  fn react_raw(
    &mut self,
    event: crossterm::event::Event,
  ) -> Result<(HandledEvent, Self::Feedback), AppError> {
    if self.input_prompt.is_visible() {
      if let (_, Some(input)) = self.input_prompt.react_raw(event)? {
        log::info!("user typed: {input}");

        if let Some(event) = self
          .input_pending_event
          .take()
          .and_then(|evt| evt.accept_input(input))
        {
          self.emit_event(event)?;
        }
      }

      return Ok((HandledEvent::handled(), ()));
    }

    match event {
      crossterm::event::Event::Resize(width, height) => {
        self.rect.width = width;
        self.rect.height = height;
        self.adjust_view();
      }

      crossterm::event::Event::Key(KeyEvent { code, .. }) => match code {
        KeyCode::Char('t') => {
          self.select_next_node();
          return Ok((HandledEvent::handled(), ()));
        }

        KeyCode::Char('s') => {
          self.select_prev_node();
          return Ok((HandledEvent::handled(), ()));
        }

        KeyCode::Char('o') => {
          self.open_prompt_insert_node("insert after:", InsertMode::After);
          return Ok((HandledEvent::handled(), ()));
        }

        KeyCode::Char('O') => {
          self.open_prompt_insert_node("insert before:", InsertMode::Before);
          return Ok((HandledEvent::handled(), ()));
        }

        KeyCode::Char('i') => {
          self.open_prompt_insert_node("insert in/bottom:", InsertMode::InsideBottom);
          return Ok((HandledEvent::handled(), ()));
        }

        KeyCode::Char('I') => {
          self.open_prompt_insert_node("insert in/top:", InsertMode::InsideTop);
          return Ok((HandledEvent::handled(), ()));
        }

        KeyCode::Char('d') => {
          self.open_prompt_delete_node();
          return Ok((HandledEvent::handled(), ()));
        }

        // ask to the node; the workflow requires to first emit an event so that the logic checks whether we should
        // open the data directly (if present), or open a menu to ask which kind of data to add
        KeyCode::Enter => {
          self.emit_event(Event::OpenNodeData {
            id: self.selected_node_id,
          })?;
        }

        KeyCode::Tab => {
          self.emit_event(Event::ToggleNode {
            id: self.selected_node_id,
          })?;
          return Ok((HandledEvent::handled(), ()));
        }

        _ => (),
      },
      _ => (),
    }

    Ok((HandledEvent::Unhandled(event), ()))
  }
}

/// Indentation state.
///
/// This state is used when rendering indent guides during tree traversal.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Indent {
  /// Current depth.
  depth: usize,

  /// Signs to use at each iteration level.
  signs: Vec<char>,
}

impl Indent {
  /// Go one level deeper in the tree, adapting the indent guides.
  ///
  /// If called on the last node in a children list, `is_last` should be set to `true`.
  fn deeper(&self, is_last: bool) -> Self {
    let sign = if is_last { ' ' } else { '│' };
    let mut signs = self.signs.clone();
    signs.push(sign);

    Self {
      depth: self.depth + 1,
      signs,
    }
  }

  /// Compute the prefix string to display before a node.
  ///
  /// The `is_last` bool parameter must be set to true whenever the node is the last one in its parent’s children list.
  fn to_indent_guides(&self, is_last: bool) -> String {
    if self.depth == 0 {
      return String::new();
    }

    let mut prefix = String::new();

    for sign in &self.signs[..self.signs.len() - 1] {
      prefix.push(*sign);
      prefix.push(' ');
    }

    if is_last {
      prefix.push_str("└ ");
    } else {
      prefix.push_str("│ ");
    }

    prefix
  }
}

/// Render the node in the given area with the given indent level, and its children.
/// Abort before rendering outside of the area (Y axis).
pub fn render_with_indent(
  node: &Node,
  top_shift: u16,
  mut id: u16,
  mut area: Rect,
  buf: &mut Buffer,
  indent: &Indent,
  is_last: bool,
  cursor: &Cursor,
) -> Option<(Rect, u16)> {
  if id >= top_shift {
    // indent guides
    let indent_guides = indent.to_indent_guides(is_last);
    buf.set_string(
      area.x,
      area.y,
      &indent_guides,
      Style::default().fg(Color::Black),
    );

    let mut render_x = indent_guides.chars().count() as u16;

    // arrow (expanded / collapsed) for nodes with children
    if node.has_children() {
      let arrow = if node.is_expanded() { " " } else { " " };
      let arrow = Span::styled(arrow, Style::default().fg(Color::Black));
      buf.set_string(render_x, area.y, &arrow.content, arrow.style);
      render_x += arrow.width() as u16;
    }

    let start_x = render_x;

    // icon rendering
    let icon = Span::styled(node.icon(), Style::default().fg(Color::Green));
    buf.set_string(render_x, area.y, &icon.content, icon.style);
    render_x += icon.width() as u16;

    // content rendering
    let text_style = Style::default();
    let text_style = if node.has_children() {
      text_style.fg(Color::Blue)
    } else {
      text_style
    };
    let text_style = if node.data().is_some() {
      text_style.add_modifier(Modifier::BOLD)
    } else {
      text_style
    };
    let text = Span::styled(node.name(), text_style);
    buf.set_string(render_x, area.y, &text.content, text.style);

    if cursor.points_to(node) {
      buf.set_style(
        Rect::new(start_x, area.y, area.width - start_x, 1),
        Style::default().bg(Color::Black),
      );
    }
  }

  // nothing else to do if we don’t have any children or they are collapsed
  if !node.has_children() || !node.is_expanded() {
    return Some((area, id));
  }

  // then render all of its children but the last (for indent quides), if any, as long as we are not going out of area
  let new_indent = indent.deeper(false);
  for child in node.children().all_except_last() {
    if id >= top_shift {
      area.y += 1;
    }

    // abort if we are at the bottom of the area
    if area.y >= area.height {
      return None;
    }

    // abort if a child hit the bottom
    let (new_area, new_id) = render_with_indent(
      child,
      top_shift,
      id + 1,
      area,
      buf,
      &new_indent,
      false,
      cursor,
    )?;

    area = new_area;
    id = new_id;
  }

  if id >= top_shift {
    // the last child is to be treated specifically for the indent sign
    area.y += 1;
  }

  // abort if we are at the bottom of the area
  if area.y >= area.height {
    return None;
  }

  if let Some(last) = node.children().last() {
    let new_indent = indent.deeper(true);
    let (new_area, new_id) = render_with_indent(
      last,
      top_shift,
      id + 1,
      area,
      buf,
      &new_indent,
      true,
      cursor,
    )?;

    area = new_area;
    id = new_id;
  }

  Some((area, id))
}
