use crossterm::{
  event::{
    DisableMouseCapture, EnableMouseCapture, KeyCode, KeyEvent, KeyEventKind, MouseButton,
    MouseEvent, MouseEventKind,
  },
  execute,
  terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
  cell::RefCell,
  io::Stdout,
  process::exit,
  rc::Rc,
  sync::mpsc::{channel, Receiver, Sender},
  thread,
  time::{Duration, Instant},
};
use thiserror::Error;
use tui::{
  backend::CrosstermBackend, buffer::Buffer, layout::Rect, style::Style, text::Span,
  widgets::Widget, Frame, Terminal,
};

fn main() {
  if let Err(err) = bootstrap() {
    eprintln!("{}", err);
    exit(1);
  }
}

fn bootstrap() -> Result<(), AppError> {
  let (event_sx, event_rx) = channel();
  let (request_sx, request_rx) = channel();

  // spawn a thread for the TUI; we can send requests to it and it sends events back to us
  let tui_thread = thread::spawn(move || {
    let tui = Tui::new(event_sx, request_rx).expect("TUI creation");
    if let Err(err) = tui.run() {
      eprintln!("TUI exited with error: {}", err);
      exit(1);
    }
  });

  // main loop of our logic application
  while let Ok(event) = event_rx.recv() {
    match event {
      Event::KeyPressed(key) => {
        if let KeyCode::Char('q') = key {
          request_sx.send(Request::Quit).unwrap();
        }
      }

      _ => (),
    }
  }

  if let Err(err) = tui_thread.join() {
    eprintln!("TUI killed while waiting for it: {:?}", err);
    exit(1);
  }

  Ok(())
}

#[derive(Debug, Error)]
enum AppError {
  #[error("initialization failed: {0}")]
  Init(std::io::Error),

  #[error("termination failed: {0}")]
  Termination(std::io::Error),

  #[error("terminal action failed: {0}")]
  TerminalAction(crossterm::ErrorKind),

  #[error("terminal event error: {0}")]
  TerminalEvent(crossterm::ErrorKind),

  #[error("TUI event error: {0}")]
  Event(String),

  #[error("rendering error: {0}")]
  Render(std::io::Error),
}

/// Event emitted in the TUI when something happens.
#[derive(Clone, Debug)]
pub enum Event {
  /// A click was performed somewhere
  Click,

  /// A key was pressed.
  KeyPressed(KeyCode),
}

/// Request sent to the TUI to make a change in it.
#[derive(Clone, Debug)]
pub enum Request {
  /// Ask the TUI to quit.
  Quit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Indent {
  depth: usize,
}

impl Indent {
  fn new() -> Self {
    Self { depth: 0 }
  }

  fn deeper(self) -> Self {
    Self {
      depth: self.depth + 1,
    }
  }

  // FIXME: something is wrong when we are the last node and we still have some nodes later; this function is probably
  // not the answer here
  /// Compute the prefix string to display before a node.
  ///
  /// The `is_last` bool parameter must be set to true whenever the node is the last one in its parent’s children list.
  fn to_indent_prefix(self, is_last: bool) -> String {
    if self.depth == 0 {
      return String::new();
    }

    let prefix = "│ ".repeat(self.depth - 1);

    if is_last {
      prefix + "└ "
    } else {
      prefix + "│ "
    }
  }
}

/// TUI version of a tree.
#[derive(Debug)]
pub struct TuiTree {
  /// Root node.
  root: TuiNode,

  /// Top node; i.e. the node that is at the top of the view.
  top_node: TuiNode,

  /// Indentation used at top-level.
  top_indent: Indent,
}

impl TuiTree {
  pub fn new(root: TuiNode) -> Self {
    let top_node = root.clone();

    Self {
      root,
      top_node,
      top_indent: Indent::new(),
    }
  }
}

impl<'a> Widget for &'a TuiTree {
  fn render(self, area: Rect, buf: &mut Buffer) {
    self
      .top_node
      .render_with_indent(area, buf, self.top_indent, false);
  }
}

/// A visual representation of a single node in the TUI.
#[derive(Clone, Debug)]
pub struct TuiNode {
  data: Rc<RefCell<TuiNodeData>>,
}

impl TuiNode {
  pub fn new(
    icon: impl Into<String>,
    text: impl Into<String>,
    children: impl Into<Vec<TuiNode>>,
  ) -> Self {
    let data = Rc::new(RefCell::new(TuiNodeData::new(icon, text, children)));
    Self { data }
  }
  /// Render the node in the given area with the given indent level, and its children.
  /// Abort before rendering outside of the area (Y axis).
  fn render_with_indent(
    &self,
    mut area: Rect,
    buf: &mut Buffer,
    indent: Indent,
    is_last: bool,
  ) -> Option<Rect> {
    // render the current node
    let data = self.data.borrow();
    let content = format!(
      "{}{}{}",
      indent.to_indent_prefix(is_last),
      data.icon,
      data.text
    ); // FIXME: last=true
    buf.set_string(area.x, area.y, content, Style::default()); // TODO: check for x boundaries?

    if data.children.is_empty() {
      return Some(area);
    }

    // then render all of its children, if any, as long as we are not going out of area
    let indent = indent.deeper();
    for child in &data.children[..data.children.len() - 1] {
      area.y += 1;

      // abort if we are at the bottom of the area
      if area.y >= area.height {
        return None;
      }

      // abort if a child hit the bottom
      area = child.render_with_indent(area, buf, indent, false)?;
    }

    // the last child is to be treated specifically for the indent sign
    area.y += 1;

    // abort if we are at the bottom of the area
    if area.y >= area.height {
      return None;
    }

    // abort if a child hit the bottom
    area = data.children[data.children.len() - 1].render_with_indent(area, buf, indent, true)?;

    Some(area)
  }
}

#[derive(Debug)]
pub struct TuiNodeData {
  icon: String,
  text: String,
  children: Vec<TuiNode>,
}

impl TuiNodeData {
  fn new(
    icon: impl Into<String>,
    text: impl Into<String>,
    children: impl Into<Vec<TuiNode>>,
  ) -> Self {
    Self {
      icon: icon.into(),
      text: text.into(),
      children: children.into(),
    }
  }
}

struct Tui {
  terminal: Terminal<CrosstermBackend<Stdout>>,
  event_sx: Sender<Event>,
  request_rx: Receiver<Request>,
}

impl Tui {
  const CLICK_TIMING: Duration = Duration::from_millis(100);

  pub fn new(event_sx: Sender<Event>, request_rx: Receiver<Request>) -> Result<Self, AppError> {
    enable_raw_mode().map_err(AppError::Init)?;

    let mut stdout = std::io::stdout();
    execute!(&mut stdout, EnterAlternateScreen, EnableMouseCapture)
      .map_err(AppError::TerminalAction)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout)).map_err(AppError::Init)?;

    terminal.hide_cursor().map_err(AppError::TerminalAction)?;

    Ok(Tui {
      terminal,
      event_sx,
      request_rx,
    })
  }

  pub fn run(mut self) -> Result<(), AppError> {
    let mut left_button_down_at = None;

    // FIXME: we start with an empty tree named Tree; we need to support something smarter
    let tree = TuiTree::new(TuiNode::new(
      " ",
      "Tree",
      [
        TuiNode::new(
          "",
          "First child",
          [TuiNode::new(
            "",
            "This should be indented",
            [
              TuiNode::new("", "Oh yeaaah", []),
              TuiNode::new("", "C’est la mouche qui pète !", []),
            ],
          )],
        ),
        TuiNode::new("", "Second child", []),
        TuiNode::new("", "Third child", []),
        TuiNode::new("", "Fourth child", []),
      ],
    ));

    loop {
      // dequeue events
      while let Ok(true) = crossterm::event::poll(Duration::from_millis(10)) {
        // event available
        let event = crossterm::event::read().map_err(AppError::TerminalEvent)?;
        match event {
          crossterm::event::Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
          }) => {
            self
              .event_sx
              .send(Event::KeyPressed(code))
              .map_err(|e| AppError::Event(e.to_string()))?;
          }

          crossterm::event::Event::Mouse(MouseEvent { kind, .. }) => match kind {
            MouseEventKind::Down(MouseButton::Left) => {
              left_button_down_at = Some(Instant::now());
            }

            MouseEventKind::Up(MouseButton::Left) => {
              match left_button_down_at {
                Some(when) if when.elapsed() <= Self::CLICK_TIMING => {
                  self
                    .event_sx
                    .send(Event::Click)
                    .map_err(|e| AppError::Event(e.to_string()))?;
                }

                _ => (),
              }

              left_button_down_at = None;
            }

            _ => (),
          },

          _ => (),
        }
      }

      // check for requests
      while let Ok(req) = self.request_rx.try_recv() {
        match req {
          Request::Quit => return Ok(()),
        }
      }

      self
        .terminal
        .draw(|f| f.render_widget(&tree, f.size()))
        .map_err(AppError::Render)?;
    }
  }
}

impl Drop for Tui {
  fn drop(&mut self) {
    let _ = self
      .terminal
      .show_cursor()
      .map_err(AppError::TerminalAction);
    let _ = execute!(
      self.terminal.backend_mut(),
      LeaveAlternateScreen,
      DisableMouseCapture
    )
    .map_err(AppError::TerminalAction);
    let _ = disable_raw_mode().map_err(AppError::Termination);
  }
}
