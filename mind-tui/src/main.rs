use crossterm::{
  event::{KeyCode, KeyEvent, KeyEventKind},
  execute,
  terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use mind_tree::{
  config::Config,
  forest::{Forest, ForestError},
  node::{Node, Tree},
};
use std::{
  io::Stdout,
  process::exit,
  str::FromStr,
  sync::{
    mpsc::{channel, Receiver, SendError, Sender},
    Arc, Mutex,
  },
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

  let (config, config_err) = Config::load_or_default();
  if let Some(config_err) = config_err {
    request_sx
      .send(Request::sticky_msg(
        format!("error while reading configuration: {}", config_err),
        Duration::from_secs(5),
      ))
      .map_err(AppError::Request)?;
  }

  // TODO: read CLI arguments to determine which tree to show; we start with the main forest for now
  let forest = Forest::from_path(
    config
      .persistence
      .forest_path()
      .ok_or(AppError::NoForestPath)?,
  )?;

  // transform the main tree into a TreeNode
  let tui_main_tree = tree_to_tui(forest.main_tree());

  // send the tree to the TUI
  request_sx
    .send(Request::NewTree(tui_main_tree))
    .map_err(AppError::Request)?;

  // main loop of our logic application
  while let Ok(event) = event_rx.recv() {
    match event {
      Event::Command(UserCmd::Quit) => request_sx.send(Request::Quit).unwrap(),

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

  #[error("error while sending a request to the TUI: {0}")]
  Request(SendError<Request>),

  #[error("rendering error: {0}")]
  Render(std::io::Error),

  #[error("unknown '{0}' command")]
  UnknownCommand(String),

  #[error("no forest path")]
  NoForestPath,

  #[error("forest error: {0}")]
  ForestError(#[from] ForestError),
}

/// Event emitted in the TUI when something happens.
#[derive(Clone, Debug)]
pub enum Event {
  /// A command was entereed.
  Command(UserCmd),
}

/// Request sent to the TUI to make a change in it.
#[derive(Debug)]
pub enum Request {
  /// Provide a new tree to display.
  NewTree(TuiTree),

  /// Display a sticky message.
  StickyMsg {
    span: Span<'static>,
    timeout: Duration,
  },

  /// Ask the TUI to quit.
  Quit,
}

impl Request {
  fn sticky_msg(span: impl Into<Span<'static>>, timeout: Duration) -> Self {
    Self::StickyMsg {
      span: span.into(),
      timeout,
    }
  }
}

/// User commands.
///
/// Those commands can be sent by typing them in the command line, for now.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum UserCmd {
  Quit,
}

impl FromStr for UserCmd {
  type Err = AppError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "q" | "quit" => Ok(UserCmd::Quit),
      _ => Err(AppError::UnknownCommand(s.to_owned())),
    }
  }
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
  data: Arc<Mutex<TuiNodeData>>,
}

impl TuiNode {
  pub fn new(
    icon: impl Into<Span<'static>>,
    text: impl Into<Span<'static>>,
    children: impl Into<Vec<TuiNode>>,
  ) -> Self {
    let data = Arc::new(Mutex::new(TuiNodeData::new(icon, text, children)));
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
    let data = self.data.lock().unwrap();

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

    // content rendering
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
  tree: TuiTree,
  sticky_msg: Option<StickyMsg>,
}

impl Tui {
  pub fn new(event_sx: Sender<Event>, request_rx: Receiver<Request>) -> Result<Self, AppError> {
    enable_raw_mode().map_err(AppError::Init)?;

    let mut stdout = std::io::stdout();
    execute!(&mut stdout, EnterAlternateScreen).map_err(AppError::TerminalAction)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout)).map_err(AppError::Init)?;

    terminal.hide_cursor().map_err(AppError::TerminalAction)?;

    let cmd_line = CmdLine::new(event_sx.clone());
    let tree = TuiTree::new(TuiNode::new("", "Mind", []));
    let sticky_msg = None;

    Ok(Tui {
      terminal,
      event_sx,
      request_rx,
      cmd_line,
      tree,
      sticky_msg,
    })
  }

  /// Allow errors to occur and display them in case of occurrence.
  pub fn display_errors<T>(&mut self, a: Result<T, AppError>, f: impl FnOnce(T)) {
    match a {
      Ok(a) => f(a),
      Err(err) => self.display_sticky(
        Span::styled(err.to_string(), Style::default().fg(Color::Red)),
        Duration::from_secs(5),
      ),
    }
  }

  pub fn run(mut self) -> Result<(), AppError> {
    let mut needs_redraw = true;
    loop {
      // event available
      let available_event = crossterm::event::poll(Duration::from_millis(50))
        .map_err(|e| AppError::Event(e.to_string()))?;

      if available_event {
        let event = crossterm::event::read().map_err(AppError::TerminalEvent)?;
        // TODO: for now we only have the command line as reactive object, so nothing specific to do with the
        // returned event if it’s unhandled
        let handled_event = self.cmd_line.react_raw(event);
        self.display_errors(handled_event, |event| {
          if let HandledEvent::Handled { requires_redraw } = event {
            needs_redraw |= requires_redraw;
          }
        });
      }

      // check for requests
      while let Ok(req) = self.request_rx.try_recv() {
        match req {
          Request::NewTree(tree) => {
            self.tree = tree;
          }

          Request::StickyMsg { span, timeout } => {
            self.display_sticky(span, timeout);
          }

          Request::Quit => return Ok(()),
        }
      }

      self.refresh();

      // render
      needs_redraw = true;
      if needs_redraw {
        self
          .terminal
          .draw(|f| {
            let size = f.size();

            // render the tree
            f.render_widget(&self.tree, size);

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

            // render any sticky message, if any
            if let Some(ref sticky_msg) = self.sticky_msg {
              let p = tui::widgets::Paragraph::new(sticky_msg.span.content.as_ref())
                .style(sticky_msg.span.style)
                .wrap(tui::widgets::Wrap { trim: false })
                .alignment(tui::layout::Alignment::Right);
              let width = size.width / 4;
              let height = size.height / 2;
              f.render_widget(
                p,
                Rect {
                  x: size.x + 3 * width,
                  y: size.y,
                  width,
                  height,
                },
              );
            }
          })
          .map_err(AppError::Render)?;
        needs_redraw = false;
      }
    }
  }

  fn refresh(&mut self) {
    // check whether we should remove sticky messages
    if let Some(ref mut sticky_msg) = self.sticky_msg {
      if sticky_msg.until < Instant::now() {
        self.sticky_msg = None;
      }
    }
  }

  /// Display a message with a given style and stick it around.
  ///
  /// The message will stick around until its timeout time is reached. If a message was still there, it is replaced
  /// with the new message.
  fn display_sticky(&mut self, span: impl Into<Span<'static>>, timeout: Duration) {
    self.sticky_msg = Instant::now()
      .checked_add(timeout)
      .map(move |until| StickyMsg::new(span, until));
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

/// Messages that won’t go out of the UI unless a given timeout is reached.
#[derive(Debug)]
pub struct StickyMsg {
  span: Span<'static>,
  until: Instant,
}

impl StickyMsg {
  fn new(span: impl Into<Span<'static>>, until: Instant) -> Self {
    Self {
      span: span.into(),
      until,
    }
  }
}

/// TUI components will react to raw events.
pub trait RawEventHandler {
  fn react_raw(&mut self, event: crossterm::event::Event) -> Result<HandledEvent, AppError>;
}

/// Handled events.
///
/// An event handler might completely consume an event (handled), or not (unhandled). In the case of a handled event,
/// it’s possible to pass more information upwards, such as whether we should render again, etc.
#[derive(Debug)]
pub enum HandledEvent {
  Unhandled(crossterm::event::Event),

  Handled { requires_redraw: bool },
}

impl HandledEvent {
  // Handled event that requires a redraw.
  fn handled() -> Self {
    Self::Handled {
      requires_redraw: true,
    }
  }
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
  fn react_raw(&mut self, event: crossterm::event::Event) -> Result<HandledEvent, AppError> {
    match self.state {
      None => {
        if let crossterm::event::Event::Key(KeyEvent {
          code: KeyCode::Char(':'),
          kind: KeyEventKind::Press,
          ..
        }) = event
        {
          self.state = Some(CmdLineState::default());
          return Ok(HandledEvent::handled());
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
              return Ok(HandledEvent::handled());
            }

            KeyCode::Enter => {
              // command line is complete
              let parsed = state.as_str().parse();
              self.state = None;

              self
                .event_sx
                .send(Event::Command(parsed?))
                .map_err(|e| AppError::Event(e.to_string()))?;

              return Ok(HandledEvent::handled());
            }

            KeyCode::Char(c) => {
              state.push_char(c);
              return Ok(HandledEvent::handled());
            }

            KeyCode::Backspace => {
              state.pop_char();
              return Ok(HandledEvent::handled());
            }

            KeyCode::Left => {
              state.move_cursor_left();
              return Ok(HandledEvent::handled());
            }

            KeyCode::Right => {
              state.move_cursor_right();
              return Ok(HandledEvent::handled());
            }

            _ => (),
          }
        }
      }
    }

    Ok(HandledEvent::Unhandled(event))
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
    self.move_cursor_right();
  }

  pub fn pop_char(&mut self) -> Option<char> {
    if self.input.is_empty() || self.cursor == 0 {
      None
    } else {
      let char = self.input.remove(self.cursor - 1);
      self.move_cursor_left();
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

fn tree_to_tui(tree: &Tree) -> TuiTree {
  TuiTree::new(root_node_to_tui(&tree.root()))
}

fn root_node_to_tui(node: &Node) -> TuiNode {
  let icon = Span::styled(
    node.icon(),
    Style::default()
      .fg(Color::Magenta)
      .add_modifier(Modifier::BOLD),
  );
  let text = Span::styled(
    node.name(),
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
  );
  let children: Vec<_> = if true || node.is_expanded() {
    node.children().into_iter().map(node_to_tui).collect()
  } else {
    Vec::new()
  };

  TuiNode::new(icon, text, children)
}

fn node_to_tui(node: &Node) -> TuiNode {
  let icon = Span::styled(node.icon(), Style::default().fg(Color::Green));

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

  let children: Vec<_> = if true || node.is_expanded() {
    node.children().into_iter().map(node_to_tui).collect()
  } else {
    Vec::new()
  };

  TuiNode::new(icon, text, children)
}
