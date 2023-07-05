use clap::Parser;
use crossterm::{
  event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
  execute,
  terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use log::LevelFilter;
use mind_tree::{
  config::Config,
  forest::{Forest, ForestError},
  node::{Cursor, Node, NodeError},
};
use simplelog::WriteLogger;
use std::{
  fs::File,
  io::Stdout,
  path::PathBuf,
  process::exit,
  str::FromStr,
  sync::mpsc::{channel, Receiver, SendError, Sender},
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

#[derive(Parser)]
pub struct Cli {
  #[clap(long)]
  log_file: Option<PathBuf>,

  #[clap(short, long, action = clap::ArgAction::Count)]
  verbose: u8,
}

impl Cli {
  fn verbosity(&self) -> LevelFilter {
    match self.verbose {
      0 => LevelFilter::Off,
      1 => LevelFilter::Error,
      2 => LevelFilter::Warn,
      3 => LevelFilter::Info,
      4 => LevelFilter::Debug,
      _ => LevelFilter::Trace,
    }
  }
}

fn main() {
  if let Err(err) = bootstrap() {
    eprintln!("{}", err);
    exit(1);
  }
}

fn bootstrap() -> Result<(), AppError> {
  let cli = Cli::parse();

  if let Some(ref log_file) = cli.log_file {
    WriteLogger::init(
      cli.verbosity(),
      simplelog::ConfigBuilder::new()
        .set_time_format_rfc3339()
        .build(),
      File::create(log_file).map_err(|err| AppError::LogFileError {
        err,
        path: log_file.to_owned(),
      })?,
    )?;

    log::info!("logger initialized and writing at {}", log_file.display());
  }

  let (config, config_err) = Config::load_or_default();

  let (event_sx, event_rx) = channel();
  let (request_sx, request_rx) = channel();

  // TODO: read CLI arguments to determine which tree to show; we start with the main forest for now
  let forest = Forest::from_path(
    config
      .persistence
      .forest_path()
      .ok_or(AppError::NoForestPath)?,
  )?;
  let main_tree = TuiTree::new(Rect::default(), event_sx.clone(), forest.main_tree().root());

  // spawn a thread for the TUI; we can send requests to it and it sends events back to us
  let tui_thread = thread::spawn(move || {
    let tui = Tui::new(main_tree, event_sx, request_rx).expect("TUI creation");
    if let Err(err) = tui.run() {
      log::error!("TUI exited with error: {}", err);
      exit(1);
    }
  });

  if let Some(config_err) = config_err {
    request_sx
      .send(Request::sticky_msg(
        format!("error while reading configuration: {}", config_err),
        Duration::from_secs(5),
      ))
      .map_err(AppError::Request)?;
  }

  // boolean representing when the tree has been modified and requires saving before quitting
  let mut dirty = false;

  // TODO: we need a dispatcher with an indirection here so that we don’t break the loop on bad events
  // main loop of our logic application
  while let Ok(event) = event_rx.recv() {
    match event {
      Event::Command(UserCmd::Quit { force }) => {
        if dirty && !force {
          request_sx
            .send(Request::warn_msg(
              "modified tree; please save or force quit (:w + :q; :q!)",
            ))
            .unwrap();
        } else {
          request_sx.send(Request::Quit).unwrap();
        }
      }

      Event::Command(UserCmd::Save) => {
        forest.persist(
          config
            .persistence
            .forest_path()
            .ok_or(AppError::NoForestPath)?,
        )?;

        dirty = false;

        request_sx.send(Request::info_msg("state saved")).unwrap();
      }

      Event::ToggleNode { id } => {
        if let Some(node) = forest.main_tree().get_node_by_line(id) {
          node.toggle_expand();
        }
      }

      Event::InsertNode { id, mode, name } => {
        log::info!("inserting node {id} {name}: {mode:?}");
        if let Some(anchor) = forest.main_tree().get_node_by_line(id) {
          let node = Node::new(name, "");
          match mode {
            InsertMode::InsideTop => anchor.insert_top(node),
            InsertMode::InsideBottom => anchor.insert_bottom(node),
            InsertMode::Before => anchor.insert_before(node)?,
            InsertMode::After => anchor.insert_after(node)?,
          }

          dirty = true;
          request_sx.send(Request::InsertedNode { id, mode }).unwrap();
        }
      }

      Event::DeleteNode { id } => {
        log::info!("deleting node {id}");
        if let Some(node) = forest.main_tree().get_node_by_line(id) {
          if let Ok(parent) = node.parent() {
            parent.delete(node)?;
            dirty = true;
            request_sx.send(Request::DeletedNode { id }).unwrap();
          } else {
            request_sx
              .send(Request::err_msg("cannot delete root node"))
              .unwrap();
          }
        }
      }
      _ => (),
    }
  }

  if let Err(err) = tui_thread.join() {
    log::error!("TUI killed while waiting for it: {:?}", err);
  }

  Ok(())
}

#[derive(Debug, Error)]
pub enum AppError {
  #[error("cannot initialize logging: {0}")]
  LoggerInit(#[from] log::SetLoggerError),

  #[error("cannot open log file {path} in write mode: {err}")]
  LogFileError { err: std::io::Error, path: PathBuf },

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

  #[error("node error: {0}")]
  NodeError(#[from] NodeError),
}

/// Event emitted in the TUI when something happens.
#[derive(Clone, Debug)]
pub enum Event {
  /// A command was entered.
  Command(UserCmd),

  /// Node selected.
  NodeSelected { id: usize },

  /// Toggle node.
  ToggleNode { id: usize },

  /// Node insertion at the current place.
  InsertNode {
    id: usize,
    mode: InsertMode,
    name: String,
  },

  /// Node deletion.
  DeleteNode { id: usize },
}

impl Event {
  fn accept_input(self, input: String) -> Option<Self> {
    match self {
      Event::InsertNode { id, mode, .. } => Some(Event::InsertNode {
        name: input,
        id,
        mode,
      }),

      Event::DeleteNode { .. } => match input.as_str() {
        "y" | "Y" => Some(self),
        _ => None,
      },

      _ => None,
    }
  }
}

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

  /// Ask the GUI to adapt to a node insertion.
  InsertedNode { id: usize, mode: InsertMode },

  /// Ask the GUI to adapt to a node deletion.
  DeletedNode { id: usize },
}

impl Request {
  fn sticky_msg(span: impl Into<Span<'static>>, timeout: Duration) -> Self {
    Self::StickyMsg {
      span: span.into(),
      timeout,
    }
  }

  fn info_msg(msg: impl Into<String>) -> Self {
    let span = Span::styled(msg.into(), Style::default().fg(Color::Blue));
    Self::sticky_msg(span, Duration::from_secs(5))
  }

  fn warn_msg(msg: impl Into<String>) -> Self {
    let span = Span::styled(msg.into(), Style::default().fg(Color::Yellow));
    Self::sticky_msg(span, Duration::from_secs(5))
  }

  fn err_msg(msg: impl Into<String>) -> Self {
    let span = Span::styled(msg.into(), Style::default().fg(Color::Red));
    Self::sticky_msg(span, Duration::from_secs(5))
  }
}

/// User commands.
///
/// Those commands can be sent by typing them in the command line, for now.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum UserCmd {
  Quit { force: bool },

  Save,
}

impl FromStr for UserCmd {
  type Err = AppError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "q" | "quit" => Ok(UserCmd::Quit { force: false }),
      "q!" | "quit!" => Ok(UserCmd::Quit { force: true }),
      "w" | "write" => Ok(UserCmd::Save),
      _ => Err(AppError::UnknownCommand(s.to_owned())),
    }
  }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Indent {
  /// Current depth.
  depth: usize,

  /// Signs to use at each iteration level.
  signs: Vec<char>,
}

impl Indent {
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

  fn emit_event(&self, event: Event) -> Result<(), AppError> {
    self
      .event_sx
      .send(event)
      .map_err(|e| AppError::Event(e.to_string()))?;
    Ok(())
  }

  fn select_prev_node(&mut self) -> bool {
    if self.cursor.visual_prev() {
      self.selected_node_id -= 1;
      self.adjust_view();
      true
    } else {
      false
    }
  }

  fn select_next_node(&mut self) -> bool {
    if self.cursor.visual_next() {
      self.selected_node_id += 1;
      self.adjust_view();
      true
    } else {
      false
    }
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

    if let Some(ref prompt) = self.input_prompt.prompt {
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
          self.emit_event(Event::NodeSelected {
            id: self.selected_node_id,
          })?;
          return Ok((HandledEvent::handled(), ()));
        }

        KeyCode::Char('s') => {
          self.select_prev_node();
          self.emit_event(Event::NodeSelected {
            id: self.selected_node_id,
          })?;
          return Ok((HandledEvent::handled(), ()));
        }

        KeyCode::Char('o') => {
          if !self.input_prompt.is_visible() {
            self.open_prompt_insert_node("insert after:", InsertMode::After);
            return Ok((HandledEvent::handled(), ()));
          }
        }

        KeyCode::Char('O') => {
          if !self.input_prompt.is_visible() {
            self.open_prompt_insert_node("insert before:", InsertMode::Before);
            return Ok((HandledEvent::handled(), ()));
          }
        }

        KeyCode::Char('i') => {
          if !self.input_prompt.is_visible() {
            self.open_prompt_insert_node("insert in/bottom:", InsertMode::InsideBottom);
            return Ok((HandledEvent::handled(), ()));
          }
        }

        KeyCode::Char('I') => {
          if !self.input_prompt.is_visible() {
            self.open_prompt_insert_node("insert in/top:", InsertMode::InsideTop);
            return Ok((HandledEvent::handled(), ()));
          }
        }

        KeyCode::Char('d') => {
          if !self.input_prompt.is_visible() {
            self.open_prompt_delete_node();
            return Ok((HandledEvent::handled(), ()));
          }
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

// TODO: check for x boundaries?
/// Render the node in the given area with the given indent level, and its children.
/// Abort before rendering outside of the area (Y axis).
fn render_with_indent(
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
      Style::default()
        .fg(Color::Black)
        .add_modifier(Modifier::DIM),
    );

    let mut render_x = indent_guides.chars().count() as u16;

    // arrow (expanded / collapsed) for nodes with children
    if node.has_children() {
      let arrow = if node.is_expanded() { " " } else { " " };
      let arrow = Span::styled(
        arrow,
        Style::default()
          .fg(Color::Black)
          .add_modifier(Modifier::DIM),
      );
      buf.set_string(render_x, area.y, &arrow.content, arrow.style);
      render_x += arrow.width() as u16;
    }

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
        Rect::new(0, area.y, area.width, 1),
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

struct Tui {
  terminal: Terminal<CrosstermBackend<Stdout>>,
  request_rx: Receiver<Request>,

  // components
  cmd_line: CmdLine,
  tree: TuiTree,
  sticky_msg: Option<StickyMsg>,
}

impl Tui {
  pub fn new(
    mut tree: TuiTree,
    event_sx: Sender<Event>,
    request_rx: Receiver<Request>,
  ) -> Result<Self, AppError> {
    enable_raw_mode().map_err(AppError::Init)?;

    let mut stdout = std::io::stdout();
    execute!(&mut stdout, EnterAlternateScreen).map_err(AppError::TerminalAction)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout)).map_err(AppError::Init)?;

    terminal.hide_cursor().map_err(AppError::TerminalAction)?;

    tree.rect = terminal.get_frame().size();

    let cmd_line = CmdLine::new(event_sx);
    let sticky_msg = None;

    Ok(Tui {
      terminal,
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
        let handled_event = self.react_raw(event).map(|(handled, _)| handled);

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
            self.tree.rect = self.terminal.get_frame().size();
          }

          Request::StickyMsg { span, timeout } => {
            self.display_sticky(span, timeout);
          }

          Request::Quit => return Ok(()),

          Request::InsertedNode { mode, .. } => {
            if let InsertMode::Before = mode {
              self.tree.selected_node_id += 1;
            }
          }

          Request::DeletedNode { .. } => {
            log::debug!("adapting TUI to deleted node…");

            // the node doesn’t exist anymore, so we try to move to the next node, or to the previous and update the
            // selected node ID
            if self.tree.select_next_node() {
              self.tree.selected_node_id -= 1;
            } else {
              self.tree.select_prev_node();
            }
          }
        }
      }

      self.refresh();

      // render
      needs_redraw = true;
      if needs_redraw {
        self.render()?;
        needs_redraw = false;
      }
    }
  }

  fn render(&mut self) -> Result<(), AppError> {
    self
      .terminal
      .draw(|f| {
        let size = f.size();

        // when the command line is active, this value contains -1 so that we do not overlap on the command line
        let mut tree_height_bias = 0;

        // render the command line, if any
        if let Some(ref prompt) = self.cmd_line.input_prompt.prompt {
          tree_height_bias = 1;

          let y = size.height - 1;

          // render the line
          f.render_widget(
            prompt,
            Rect {
              y,
              height: 1,
              ..size
            },
          );

          // position the cursor
          f.set_cursor(size.x + 1 + prompt.cursor() as u16, y);
        }

        // render the tree
        f.render_widget(
          &self.tree,
          Rect {
            height: self.tree.rect.height - tree_height_bias,
            ..self.tree.rect
          },
        );

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
      .map(|_| ())
      .map_err(AppError::Render)
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

impl RawEventHandler for Tui {
  type Feedback = ();

  fn react_raw(
    &mut self,
    event: crossterm::event::Event,
  ) -> Result<(HandledEvent, Self::Feedback), AppError> {
    let handled = self
      .cmd_line
      .react_raw(event)
      .map(|(handled, _)| handled)?
      .and_then(&mut self.tree)?;
    Ok((handled, ()))
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
  /// Feedback value returned by child to their parent.
  type Feedback;

  fn react_raw(
    &mut self,
    event: crossterm::event::Event,
  ) -> Result<(HandledEvent, Self::Feedback), AppError>;
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

  fn and_then<T>(self, next_handler: &mut T) -> Result<HandledEvent, AppError>
  where
    T: RawEventHandler,
  {
    if let HandledEvent::Unhandled(event) = self {
      next_handler.react_raw(event).map(|(evt, _)| evt)
    } else {
      Ok(self)
    }
  }
}

#[derive(Debug)]
pub struct CmdLine {
  input_prompt: UserInputPrompt,
  event_sx: Sender<Event>,
}

impl CmdLine {
  fn new(event_sx: Sender<Event>) -> Self {
    let input_prompt = UserInputPrompt::new_with_completions([
      "q".to_owned(),
      "q!".to_owned(),
      "quit".to_owned(),
      "quit!".to_owned(),
      "w".to_owned(),
      "write".to_owned(),
    ]);

    Self {
      input_prompt,
      event_sx,
    }
  }
}

impl RawEventHandler for CmdLine {
  type Feedback = ();

  fn react_raw(
    &mut self,
    event: crossterm::event::Event,
  ) -> Result<(HandledEvent, Self::Feedback), AppError> {
    if self.input_prompt.is_visible() {
      let (handled, input) = self.input_prompt.react_raw(event)?;

      if let Some(input) = input {
        // command line is complete
        let usr_cmd = input.parse()?;

        self
          .event_sx
          .send(Event::Command(usr_cmd))
          .map_err(|e| AppError::Event(e.to_string()))?;
      }

      return Ok((handled, ()));
    } else if let crossterm::event::Event::Key(KeyEvent {
      code: KeyCode::Char(':'),
      kind: KeyEventKind::Press,
      ..
    }) = event
    {
      self.input_prompt.show();
      return Ok((HandledEvent::handled(), ()));
    }

    Ok((HandledEvent::Unhandled(event), ()))
  }
}

/// Component displaying an input prompt to ask data from the user.
#[derive(Debug, Default)]
pub struct UserInputPrompt {
  prompt: Option<InputPrompt>,
  completions: Vec<String>,
}

impl UserInputPrompt {
  fn new_with_completions(completions: impl Into<Vec<String>>) -> Self {
    Self {
      prompt: None,
      completions: completions.into(),
    }
  }

  fn is_visible(&self) -> bool {
    self.prompt.is_some()
  }

  fn show(&mut self) {
    let prompt = InputPrompt {
      completions: self.completions.clone(),
      ..InputPrompt::default()
    };

    self.prompt = Some(prompt);
  }

  fn show_with_title(&mut self, title: impl Into<String>) {
    let prompt = InputPrompt {
      completions: self.completions.clone(),
      title: title.into(),
      ..InputPrompt::default()
    };
    self.prompt = Some(prompt);
  }

  fn hide(&mut self) {
    self.prompt = None;
  }
}

impl RawEventHandler for UserInputPrompt {
  type Feedback = Option<String>;

  fn react_raw(
    &mut self,
    event: crossterm::event::Event,
  ) -> Result<(HandledEvent, Self::Feedback), AppError> {
    if let Some(ref mut input_prompt) = self.prompt {
      if let crossterm::event::Event::Key(KeyEvent {
        code,
        kind: KeyEventKind::Press,
        ..
      }) = event
      {
        match code {
          KeyCode::Esc => {
            self.hide();
            return Ok((HandledEvent::handled(), None));
          }

          KeyCode::Enter => {
            // command line is complete
            let input = self.prompt.take().map(|prompt| prompt.as_str().to_owned());

            return Ok((HandledEvent::handled(), input));
          }

          KeyCode::Char(c) => {
            input_prompt.push_char(c);
            return Ok((HandledEvent::handled(), None));
          }

          KeyCode::Backspace => {
            input_prompt.pop_char();
            return Ok((HandledEvent::handled(), None));
          }

          KeyCode::Left => {
            input_prompt.move_cursor_left();
            return Ok((HandledEvent::handled(), None));
          }

          KeyCode::Right => {
            input_prompt.move_cursor_right();
            return Ok((HandledEvent::handled(), None));
          }

          _ => (),
        }
      }
    }

    Ok((HandledEvent::Unhandled(event), None))
  }
}

#[derive(Debug)]
pub struct InputPrompt {
  input: String,
  cursor: usize,
  title: String,
  completions: Vec<String>,
}

impl Default for InputPrompt {
  fn default() -> Self {
    Self {
      input: String::default(),
      cursor: 0,
      title: ":".to_owned(),
      completions: Vec::new(),
    }
  }
}

impl InputPrompt {
  fn to_byte_pos(&self, cursor: usize) -> usize {
    self.input.chars().take(cursor).map(char::len_utf8).sum()
  }

  fn push_char(&mut self, c: char) {
    let index = self.to_byte_pos(self.cursor);
    self.input.insert(index, c);
    self.move_cursor_right();
  }

  fn pop_char(&mut self) -> Option<char> {
    if self.input.is_empty() || self.cursor == 0 {
      None
    } else {
      let index = self.to_byte_pos(self.cursor - 1);
      let char = self.input.remove(index);
      self.move_cursor_left();
      Some(char)
    }
  }

  fn move_cursor_left(&mut self) {
    self.cursor = self.cursor.saturating_sub(1);
  }

  fn move_cursor_right(&mut self) {
    self.cursor = self.cursor.min(self.input.len() - 1) + 1;
  }

  fn cursor(&self) -> usize {
    self.cursor
  }

  fn as_str(&self) -> &str {
    self.input.as_str()
  }
}

impl<'a> Widget for &'a InputPrompt {
  fn render(self, area: Rect, buf: &mut Buffer) {
    // render the prefix grey with no text; green if the function is valid and red if not
    let input_str = self.input.as_str();
    let color = if self.completions.is_empty() {
      Color::Blue
    } else if self.completions.iter().any(|c| c == input_str) {
      Color::Green
    } else {
      Color::Red
    };

    buf.set_string(area.x, area.y, &self.title, Style::default().fg(color));
    buf.set_string(
      area.x + self.title.len() as u16,
      area.y,
      &self.input,
      Style::default()
        .fg(Color::Magenta)
        .remove_modifier(Modifier::all()),
    );
  }
}

/// An option menu.
///
/// This menu will display a list of selections and will return the one the user selected, or [`None`] if the menu is
/// aborted.
#[derive(Debug, Default)]
pub struct Menu {
  items: Vec<MenuItem>,
  currently_selected: usize, // index in items
}

impl Menu {
  pub fn new(items: impl Into<Vec<MenuItem>>) -> Self {
    Self {
      items: items.into(),
      currently_selected: 0,
    }
  }

  pub fn select_prev(&mut self) {
    self.currently_selected = self.currently_selected.max(1) - 1;
  }

  pub fn select_next(&mut self) {
    self.currently_selected = (self.items.len() - 1).min(self.currently_selected + 1);
  }

  pub fn selected(&self) -> Option<&str> {
    self
      .items
      .get(self.currently_selected)
      .map(|item| item.name.as_str())
  }
}

impl RawEventHandler for Menu {
  type Feedback = Option<MenuItem>;

  fn react_raw(
    &mut self,
    event: crossterm::event::Event,
  ) -> Result<(HandledEvent, Self::Feedback), AppError> {
    match event {
      // select previous item
      crossterm::event::Event::Key(KeyEvent {
        code: KeyCode::Char('p'),
        modifiers: KeyModifiers::CONTROL,
        ..
      }) => {
        self.select_prev();
        Ok((HandledEvent::handled(), None))
      }

      // select next item
      crossterm::event::Event::Key(KeyEvent {
        code: KeyCode::Char('n'),
        modifiers: KeyModifiers::CONTROL,
        ..
      }) => {
        self.select_next();
        Ok((HandledEvent::handled(), None))
      }

      // select item directly by pressing a key
      crossterm::event::Event::Key(KeyEvent {
        code: KeyCode::Char(c),
        ..
      }) => {
        if let Some((i, item)) = self
          .items
          .iter()
          .enumerate()
          .find(|(_, item)| item.key == Some(c))
        {
          self.currently_selected = i;
          return Ok((HandledEvent::handled(), Some(item.clone())));
        }

        Ok((HandledEvent::Unhandled(event), None))
      }

      _ => Ok((HandledEvent::Unhandled(event), None)),
    }
  }
}

impl<'a> Widget for &'a Menu {
  fn render(self, area: Rect, buf: &mut Buffer) {
    // center on area.width based on the longest item
    //let longest_width = self.items.iter().map(|item| item.name.len()).max();

    for (i, item) in self.items.iter().enumerate() {
      let mut x = area.x;

      // if there is a key set, print it first and shift x by the length
      if let Some(key) = item.key {
        let s = format!("({key}) ");
        buf.set_string(x, area.y + i as u16, &s, Style::default());
        x += s.len() as u16;
      }

      // then just render the actual menu item
      buf.set_string(x, area.y + i as u16, &item.name, Style::default());

      // highlight if selected
      if i == self.currently_selected {
        buf.set_style(
          Rect {
            height: 1,
            y: area.y + i as u16,
            ..area
          },
          Style::default()
            .bg(Color::Black)
            .add_modifier(Modifier::DIM),
        );
      }
    }
  }
}

/// A menu item.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MenuItem {
  name: String,
  key: Option<char>,
}

impl MenuItem {
  pub fn new(name: impl Into<String>, key: impl Into<Option<char>>) -> Self {
    Self {
      name: name.into(),
      key: key.into(),
    }
  }
}

impl<S> From<S> for MenuItem
where
  S: Into<String>,
{
  fn from(value: S) -> Self {
    MenuItem {
      name: value.into(),
      key: None,
    }
  }
}
