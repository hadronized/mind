mod cli;
mod components;
mod error;
mod event;
mod ops;
mod req;

use clap::Parser;
use cli::Cli;
use components::{tree::TuiTree, tui::Tui};
use crossterm::{
  execute,
  terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use error::AppError;
use event::Event;
use mind_tree::{
  config::Config,
  forest::Forest,
  node::{Node, NodeData},
};
use ops::InsertMode;
use req::{Request, UserCmd};
use simplelog::WriteLogger;
use std::{
  fs::File,
  io::stdout,
  path::Path,
  process::{exit, Command, Stdio},
  sync::mpsc::{channel, Receiver, Sender},
  thread::{self, JoinHandle},
  time::Duration,
};
use tui::layout::Rect;

fn main() {
  if let Err(err) = bootstrap() {
    eprintln!("{}", err);
    exit(1);
  }
}

fn bootstrap() -> Result<(), AppError> {
  let app = App::init()?;
  app.dispatch_events()
}

#[derive(Debug)]
struct App {
  config: Config,
  tui_data: TuiData,
  forest: Forest,
  dirty: bool,
}

#[derive(Debug)]
struct TuiData {
  event_rx: Receiver<Event>,
  request_sx: Sender<Request>,
  tui_handle: JoinHandle<()>,
}

impl App {
  fn init_logger(cli: &Cli) -> Result<(), AppError> {
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

    Ok(())
  }

  fn init() -> Result<Self, AppError> {
    let cli = Cli::parse();

    Self::init_logger(&cli)?;

    let (config, config_err) = Config::load_or_default();

    let (event_sx, event_rx) = channel();

    // TODO: read CLI arguments to determine which tree to show; we start with the main forest for now
    let forest = Forest::from_path(
      config
        .persistence
        .forest_path()
        .ok_or(AppError::NoForestPath)?,
    )?;
    let main_tree = TuiTree::new(Rect::default(), event_sx.clone(), forest.main_tree().root());

    let tui_data = Self::spawn_tui(main_tree, event_sx, event_rx)?;

    if let Some(config_err) = config_err {
      tui_data
        .request_sx
        .send(Request::sticky_msg(
          format!("error while reading configuration: {}", config_err),
          Duration::from_secs(5),
        ))
        .map_err(AppError::Request)?;
    }

    // boolean representing when the tree has been modified and requires saving before quitting
    let dirty = false;

    Ok(Self {
      config,
      tui_data,
      forest,
      dirty,
    })
  }

  /// Spawn the TUI.
  ///
  /// `event_sx` and `event_rx` are both ends of a channel used to communicate between the TUI and the calling code.
  fn spawn_tui(
    tree: TuiTree,
    event_sx: Sender<Event>,
    event_rx: Receiver<Event>,
  ) -> Result<TuiData, AppError> {
    let (request_sx, request_rx) = channel();

    let tui_handle = thread::spawn(move || {
      let tui = Tui::new(tree, event_sx, request_rx).expect("TUI creation");
      if let Err(err) = tui.run() {
        log::error!("TUI exited with error: {}", err);
        exit(1);
      }
    });

    Ok(TuiData {
      event_rx,
      request_sx,
      tui_handle,
    })
  }

  /// Send a request to the TUI.
  fn request(&self, req: Request) -> Result<(), AppError> {
    self.tui_data.request_sx.send(req).map_err(AppError::from)
  }

  /// Wait and dispatch incoming events from the TUI.
  fn dispatch_events(mut self) -> Result<(), AppError> {
    // main loop of our logic application
    while let Ok(event) = self.tui_data.event_rx.recv() {
      match event {
        Event::Command(usr_cmd) => self.on_user_cmd(usr_cmd)?,
        Event::ToggleNode { id } => self.on_toggle_node(id)?,
        Event::InsertNode { id, mode, name } => self.on_insert_node(id, mode, name)?,
        Event::DeleteNode { id } => self.on_delete_node(id)?,
        Event::OpenNodeData { id } => self.on_open_node_data(id)?,
      }
    }

    if let Err(err) = self.tui_data.tui_handle.join() {
      log::error!("TUI killed while waiting for it: {:?}", err);
    }

    Ok(())
  }

  fn on_user_cmd(&mut self, cmd: UserCmd) -> Result<(), AppError> {
    match cmd {
      UserCmd::Quit { force } => {
        if self.dirty && !force {
          self.request(Request::warn_msg(
            "modified tree; please save or force quit (:w + :q; :q!)",
          ))?;
        } else {
          self.request(Request::Quit)?;
        }
      }

      UserCmd::Save => {
        self.forest.persist(
          self
            .config
            .persistence
            .forest_path()
            .ok_or(AppError::NoForestPath)?,
        )?;

        self.dirty = false;
        self.request(Request::info_msg("state saved"))?;
      }
    }

    Ok(())
  }

  fn on_toggle_node(&mut self, id: usize) -> Result<(), AppError> {
    if let Some(node) = self.forest.main_tree().get_node_by_line(id) {
      node.toggle_expand();
    }

    Ok(())
  }

  fn on_insert_node(&mut self, id: usize, mode: InsertMode, name: String) -> Result<(), AppError> {
    log::info!("inserting node {id} {name}: {mode:?}");

    if let Some(anchor) = self.forest.main_tree().get_node_by_line(id) {
      let node = Node::new(name, "");
      match mode {
        InsertMode::InsideTop => anchor.insert_top(node),
        InsertMode::InsideBottom => anchor.insert_bottom(node),
        InsertMode::Before => anchor.insert_before(node)?,
        InsertMode::After => anchor.insert_after(node)?,
      }

      self.dirty = true;
      self.request(Request::InsertedNode { id, mode })?;
    }

    Ok(())
  }

  fn on_delete_node(&mut self, id: usize) -> Result<(), AppError> {
    log::info!("deleting node {id}");

    if let Some(node) = self.forest.main_tree().get_node_by_line(id) {
      if let Ok(parent) = node.parent() {
        parent.delete(node)?;
        self.request(Request::DeletedNode { id })?;
      } else {
        self.request(Request::err_msg("cannot delete root node"))?;
      }
    }

    Ok(())
  }

  fn on_open_node_data(&mut self, id: usize) -> Result<(), AppError> {
    if let Some(node) = self.forest.main_tree().get_node_by_line(id) {
      match node.data() {
        Some(data) => self.open_node_data(&data)?,
        None => self.request_prompt_node_data()?,
      }
    }

    Ok(())
  }

  fn open_node_data(&mut self, data: &NodeData) -> Result<(), AppError> {
    match data {
      NodeData::File(path) => self.open_node_file(path),
      NodeData::Link(url) => self.open_node_link(url),
    }
  }

  fn open_node_file(&self, path: &Path) -> Result<(), AppError> {
    log::info!("opening node path {}", path.display());

    // get the editor to use to open the file
    let editor = self
      .config
      .ui
      .editor
      .as_ref()
      .cloned()
      .or_else(|| std::env::var("EDITOR").ok())
      .ok_or_else(|| AppError::NodePathOpenError {
        path: path.to_owned(),
        err: "no editor configured".to_owned(),
      })?;

    log::debug!("with editor {editor}");

    // TODO: we must leave raw mode here; careful to the fact this function might fail
    let mut stdout = stdout();
    execute!(stdout, LeaveAlternateScreen).map_err(AppError::TerminalAction)?;
    let res = Command::new(editor)
      .arg(path)
      .status()
      .map_err(|err| AppError::NodePathOpenError {
        path: path.to_owned(),
        err: format!("error while opening editor: {}", err),
      });
    execute!(stdout, EnterAlternateScreen).map_err(AppError::TerminalAction)?;
    let _ = res?;

    Ok(())
  }

  fn open_node_link(&self, url: &str) -> Result<(), AppError> {
    log::info!("opening URL {url}");

    open::that(url).map_err(|err| AppError::URLOpenError {
      url: url.to_owned(),
      err: err.to_string(),
    })
  }

  fn request_prompt_node_data(&mut self) -> Result<(), AppError> {
    let (sender, rx) = channel();
    self.request(Request::PromptNodeData { sender })?;

    // wait for the TUI to reply with something
    if let Ok(resp) = rx.recv() {
      log::info!("user wants to create {resp:?}");
    }

    Ok(())
  }
}
