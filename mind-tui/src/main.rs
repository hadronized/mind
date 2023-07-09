mod cli;
mod components;
mod error;
mod event;
mod ops;
mod req;

use clap::Parser;
use cli::Cli;
use components::{tree::TuiTree, tui::Tui};
use error::AppError;
use event::Event;
use mind_tree::{config::Config, forest::Forest, node::Node};
use ops::InsertMode;
use req::{Request, UserCmd};
use simplelog::WriteLogger;
use std::{
  fs::File,
  process::exit,
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
      if node.data().is_none() {
        let (sender, rx) = channel();
        self.request(Request::PromptNodeData { sender })?;

        // wait for the TUI to reply with something
        if let Ok(resp) = rx.recv() {
          log::info!("user wants to create {resp:?}");
        }
      }
    }

    Ok(())
  }
}
