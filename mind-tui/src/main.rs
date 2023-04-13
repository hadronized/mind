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
  backend::CrosstermBackend,
  buffer::Buffer,
  layout::Rect,
  style::{Color, Modifier, Style},
  text::Span,
  widgets::Widget,
  Terminal,
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

      Event::Command(cmd) if ["q", "quit"].contains(&cmd.as_str()) => {
        request_sx.send(Request::Quit).unwrap()
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
pub enum AppError {
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
  /// A command was entereed.
  Command(String),

  /// A key was pressed.
  KeyPressed(KeyCode),
}

/// Request sent to the TUI to make a change in it.
#[derive(Clone, Debug)]
pub enum Request {
  /// Ask the TUI to quit.
  Quit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Indent {
  /// Current depth.
  depth: usize,

  /// Signs to use at each iteration level.
  signs: Vec<char>,
}

impl Indent {
  fn new() -> Self {
    Self {
      depth: 0,
      signs: Vec::new(),
    }
  }

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
      .render_with_indent(area, buf, &self.top_indent, false);
  }
}

/// A visual representation of a single node in the TUI.
#[derive(Clone, Debug)]
pub struct TuiNode {
  data: Rc<RefCell<TuiNodeData>>,
}

impl TuiNode {
  pub fn new(
    icon: impl Into<Span<'static>>,
    text: impl Into<Span<'static>>,
    children: impl Into<Vec<TuiNode>>,
  ) -> Self {
    let data = Rc::new(RefCell::new(TuiNodeData::new(icon, text, children)));
    Self { data }
  }

  // TODO: check for x boundaries?
  /// Render the node in the given area with the given indent level, and its children.
  /// Abort before rendering outside of the area (Y axis).
  fn render_with_indent(
    &self,
    mut area: Rect,
    buf: &mut Buffer,
    indent: &Indent,
    is_last: bool,
  ) -> Option<Rect> {
    // render the current node
    let data = self.data.borrow();

    // indent guides
    let indent_guides = indent.to_indent_guides(is_last);
    buf.set_string(
      area.x,
      area.y,
      &indent_guides,
      Style::default()
        .fg(Color::Black)
        .add_modifier(Modifier::DIM),
    );

    let mut render_x = indent_guides.chars().count() as u16;

    // icon rendering
    buf.set_string(render_x, area.y, &data.icon.content, data.icon.style);
    render_x += data.icon.width() as u16;

    // context rendering
    buf.set_string(render_x, area.y, &data.text.content, data.text.style);

    // traverse the children
    if data.children.is_empty() {
      return Some(area);
    }

    // then render all of its children, if any, as long as we are not going out of area
    let new_indent = indent.deeper(false);
    for child in &data.children[..data.children.len() - 1] {
      area.y += 1;

      // abort if we are at the bottom of the area
      if area.y >= area.height {
        return None;
      }

      // abort if a child hit the bottom
      area = child.render_with_indent(area, buf, &new_indent, false)?;
    }

    // the last child is to be treated specifically for the indent sign
    area.y += 1;

    // abort if we are at the bottom of the area
    if area.y >= area.height {
      return None;
    }

    // abort if a child hit the bottom
    let new_indent = indent.deeper(true);
    area =
      data.children[data.children.len() - 1].render_with_indent(area, buf, &new_indent, true)?;

    Some(area)
  }
}

#[derive(Debug)]
pub struct TuiNodeData {
  icon: Span<'static>,
  text: Span<'static>,
  children: Vec<TuiNode>,
}

impl TuiNodeData {
  fn new(
    icon: impl Into<Span<'static>>,
    text: impl Into<Span<'static>>,
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

  // components
  cmd_line: CmdLine,
}

impl Tui {
  pub fn new(event_sx: Sender<Event>, request_rx: Receiver<Request>) -> Result<Self, AppError> {
    enable_raw_mode().map_err(AppError::Init)?;

    let mut stdout = std::io::stdout();
    execute!(&mut stdout, EnterAlternateScreen).map_err(AppError::TerminalAction)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout)).map_err(AppError::Init)?;

    terminal.hide_cursor().map_err(AppError::TerminalAction)?;

    let cmd_line = CmdLine::new(event_sx.clone());

    Ok(Tui {
      terminal,
      event_sx,
      request_rx,
      cmd_line,
    })
  }

  pub fn run(mut self) -> Result<(), AppError> {
    // FIXME: we start with an empty tree named Tree; we need to support something smarter
    let tree = TuiTree::new(TuiNode::new(
      Span::styled(
        " ",
        Style::default()
          .add_modifier(Modifier::BOLD)
          .fg(Color::Blue),
      ),
      Span::styled(
        "Tree",
        Style::default()
          .add_modifier(Modifier::BOLD)
          .fg(Color::Magenta),
      ),
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
        TuiNode::new(
          Span::styled(
            "A ",
            Style::default()
              .add_modifier(Modifier::BOLD)
              .fg(Color::Green),
          ),
          "Second child",
          [],
        ),
        TuiNode::new("B ", "Third child", []),
        TuiNode::new("C ", "Fourth child", []),
      ],
    ));

    loop {
      // event available
      let available_event = crossterm::event::poll(Duration::from_millis(50))
        .map_err(|e| AppError::Event(e.to_string()))?;

      if available_event {
        let event = crossterm::event::read().map_err(AppError::TerminalEvent)?;
        // TODO: for now we only have the command line as reactive object, so nothing specific to do with the
        // returned event if it’s unhandled
        let _ = self.cmd_line.react_raw(event)?;
      }

      // check for requests
      while let Ok(req) = self.request_rx.try_recv() {
        match req {
          Request::Quit => return Ok(()),
        }
      }

      // render
      self
        .terminal
        .draw(|f| {
          let size = f.size();
          // render the tree
          f.render_widget(&tree, size);

          // render the command line, if any
          if let Some(ref state) = self.cmd_line.state {
            let y = size.height - 1;

            // render the line
            f.render_widget(
              state,
              Rect {
                y,
                height: 1,
                ..size
              },
            );

            // position the cursor
            f.set_cursor(size.x + 1 + state.cursor() as u16, y);
          }
        })
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
    let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen,)
      .map_err(AppError::TerminalAction);
    let _ = disable_raw_mode().map_err(AppError::Termination);
  }
}

/// TUI components will react to raw events.
pub trait RawEventHandler {
  fn react_raw(
    &mut self,
    event: crossterm::event::Event,
  ) -> Result<Option<crossterm::event::Event>, AppError>;
}

#[derive(Debug)]
pub struct CmdLine {
  state: Option<CmdLineState>,
  event_sx: Sender<Event>,
}

impl CmdLine {
  fn new(event_sx: Sender<Event>) -> Self {
    Self {
      state: None,
      event_sx,
    }
  }
}

impl RawEventHandler for CmdLine {
  fn react_raw(
    &mut self,
    event: crossterm::event::Event,
  ) -> Result<Option<crossterm::event::Event>, AppError> {
    match self.state {
      None => {
        if let crossterm::event::Event::Key(KeyEvent {
          code: KeyCode::Char(':'),
          kind: KeyEventKind::Press,
          ..
        }) = event
        {
          self.state = Some(CmdLineState::default());
          return Ok(None);
        }
      }

      Some(ref mut state) => {
        if let crossterm::event::Event::Key(KeyEvent {
          code,
          kind: KeyEventKind::Press,
          ..
        }) = event
        {
          match code {
            KeyCode::Esc => {
              // disable  the command line
              self.state = None;
              return Ok(None);
            }

            KeyCode::Enter => {
              // command line is complete
              self
                .event_sx
                .send(Event::Command(state.as_str().to_owned()))
                .map_err(|e| AppError::Event(e.to_string()))?;

              self.state = None;
              return Ok(None);
            }

            KeyCode::Char(c) => {
              state.push_char(c);
              return Ok(None);
            }

            KeyCode::Backspace => {
              state.pop_char();
              return Ok(None);
            }

            KeyCode::Left => {
              state.move_cursor_left();
              return Ok(None);
            }

            KeyCode::Right => {
              state.move_cursor_right();
              return Ok(None);
            }

            _ => (),
          }
        }
      }
    }

    Ok(Some(event))
  }
}

#[derive(Debug, Default)]
pub struct CmdLineState {
  input: String,
  cursor: usize,
}

impl CmdLineState {
  pub fn push_char(&mut self, c: char) {
    self.input.insert(self.cursor, c);
    self.cursor += 1;
  }

  pub fn pop_char(&mut self) -> Option<char> {
    if self.input.is_empty() {
      None
    } else {
      let char = self.input.remove(self.cursor - 1);
      self.cursor -= 1;
      Some(char)
    }
  }

  pub fn move_cursor_left(&mut self) {
    self.cursor = self.cursor.max(1) - 1;
  }

  pub fn move_cursor_right(&mut self) {
    self.cursor = self.cursor.min(self.input.len() - 1) + 1;
  }

  pub fn cursor(&self) -> usize {
    self.cursor
  }

  pub fn as_str(&self) -> &str {
    self.input.as_str()
  }
}

impl<'a> Widget for &'a CmdLineState {
  fn render(self, area: Rect, buf: &mut Buffer) {
    buf.set_string(area.x, area.y, ":", Style::default().fg(Color::Magenta));
    buf.set_string(area.x + 1, area.y, &self.input, Style::default());
  }
}
