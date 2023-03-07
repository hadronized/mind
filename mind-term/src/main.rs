mod cli;
mod config;
mod data_file;
mod ui;

use clap::Parser;
use cli::{Command, DataCommand, DataType, InsertMode, CLI};
use colored::Colorize;
use config::Config;
use data_file::{DataFileStore, DataFileStoreError};
use mind::forest::Forest;
use mind::node::{Node, NodeData, NodeError, NodeFilter};
use mind::{encoding, node::Tree};
use std::borrow::Cow;
use std::env::current_dir;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::{fs, io};
use thiserror::Error;
use ui::{UIError, UI};

const PROJECT_ICON: &'static str = " ";

/// The top-level type holding everything that the application is about.
struct App {
  config: Config,
  cli: CLI,
  ui: UI,
  data_file_store: DataFileStore,
}

impl App {
  fn new() -> Result<Self, PutainDeMerdeError> {
    let config = Self::load_config();
    let cli = CLI::parse();
    let ui = UI::new(&config);
    let data_dir = config
      .persistence
      .data_dir()
      .ok_or(PutainDeMerdeError::NoDataDir)?;
    let data_file_store = DataFileStore::new(data_dir);

    Ok(Self {
      config,
      cli,
      ui,
      data_file_store,
    })
  }

  fn load_config() -> Config {
    // TODO: get config from env var / XDG / whatever
    let (config, config_err) = Config::load_or_default();
    if let Some(config_err) = config_err {
      eprintln!(
        "{}",
        format!("error while reading configuration: {}", config_err).red()
      );
    }

    config
  }

  fn load_tree(path: impl AsRef<Path>) -> Result<Tree, PutainDeMerdeError> {
    let path = path.as_ref();

    if !path.exists() {
      return Err(PutainDeMerdeError::NoTreePersisted);
    }

    let tree: encoding::Tree =
      serde_json::from_str(&fs::read_to_string(path).map_err(PutainDeMerdeError::CannotReadTree)?)
        .map_err(PutainDeMerdeError::CannotDeserializeTree)?;
    Ok(Tree::from_encoding(tree))
  }

  fn load_forest(&self) -> Result<Forest, PutainDeMerdeError> {
    let path = self
      .config
      .persistence
      .forest_path()
      .ok_or(PutainDeMerdeError::NoForestPath)?;

    if !path.exists() {
      return Err(PutainDeMerdeError::NoForestPersisted);
    }

    let contents = fs::read_to_string(path).map_err(PutainDeMerdeError::CannotReadForest)?;
    serde_json::from_str(&contents).map_err(PutainDeMerdeError::CannotDeserializeForest)
  }

  fn persist_tree_to_path(tree: &Tree, path: impl AsRef<Path>) -> Result<(), PutainDeMerdeError> {
    let path = path.as_ref();

    // ensure all parent directories are created
    match path.parent() {
      Some(parent) => {
        fs::create_dir_all(parent).map_err(PutainDeMerdeError::CannotCreateDirectories)?;
      }

      _ => (),
    }

    let serialized =
      serde_json::to_string(tree).map_err(PutainDeMerdeError::CannotSerializeTree)?;
    fs::write(path, serialized).map_err(PutainDeMerdeError::CannotWriteTree)?;
    Ok(())
  }

  fn persist_forest(&self, forest: &Forest) -> Result<(), PutainDeMerdeError> {
    let path = self
      .config
      .persistence
      .forest_path()
      .ok_or(PutainDeMerdeError::NoForestPath)?;

    // ensure all parent directories are created
    match path.parent() {
      Some(parent) => {
        fs::create_dir_all(parent).map_err(PutainDeMerdeError::CannotCreateDirectories)?;
      }

      _ => (),
    }

    let serialized =
      serde_json::to_string(forest).map_err(PutainDeMerdeError::CannotSerializeForest)?;
    fs::write(path, serialized).map_err(PutainDeMerdeError::CannotWriteForest)?;
    Ok(())
  }

  /// Get the path to a local mind in the cwd argument.
  fn local_mind_path(cwd: impl AsRef<Path>) -> PathBuf {
    cwd.as_ref().join(".mind/state.json")
  }

  /// Start the application by adding an error handler layer.
  fn bootstrap() {
    match Self::new().and_then(Self::run) {
      Err(err) => {
        eprintln!("{}", err.to_string().red());
      }

      _ => (),
    }
  }

  /// Start and dispatch the application by looking at the CLI, config, etc.
  fn run(self) -> Result<(), PutainDeMerdeError> {
    match &self.cli.cmd {
      Command::Init { name } => self.run_init_cmd(name),
      Command::Insert { mode, sel, name } => {
        self.run_insert_cmd(*mode, sel.as_deref(), name.as_deref())
      }
      Command::Remove { sel } => self.run_remove_cmd(sel.as_deref()),
      Command::Rename { sel, name } => self.run_rename_cmd(sel.as_deref(), name.as_deref()),
      Command::Icon { sel, icon } => self.run_icon_cmd(sel.as_deref(), icon.as_deref()),
      Command::Move { mode, sel, dest } => {
        self.run_move_cmd(*mode, sel.as_deref(), dest.as_deref())
      }
      // TODO: add filtering
      Command::Paths { sel, ty } => self.run_paths_cmd(sel.as_deref(), *ty),
      Command::Data { sel, ty, open, cmd } => self.run_data_cmd(sel.as_deref(), *ty, *open, cmd),
    }
  }

  fn get_tree(&self) -> Result<AppTree, PutainDeMerdeError> {
    match self.cli.path {
      Some(ref tree_path) => Self::load_tree(tree_path).map(|tree| AppTree::Specific {
        path: tree_path.to_owned(),
        tree,
      }),

      None => {
        let cwd = current_dir().map_err(PutainDeMerdeError::NoCWD)?;

        if self.cli.local {
          let path = Self::local_mind_path(&cwd);
          Self::load_tree(&path).map(|tree| AppTree::Specific { path, tree })
        } else if self.cli.cwd {
          let forest = self.load_forest()?;
          forest
            .cwd_tree(cwd.clone())
            .cloned()
            .map(|tree| AppTree::Forest {
              forest,
              tree: tree.clone(),
            })
            .ok_or_else(|| PutainDeMerdeError::NoCWDTree(cwd))
        } else {
          self.load_forest().map(|forest| AppTree::Forest {
            tree: forest.main_tree().clone(),
            forest,
          })
        }
      }
    }
  }

  /// Persist the application tree.
  fn persist(&self, tree: AppTree) -> Result<(), PutainDeMerdeError> {
    match tree {
      AppTree::Specific { path, tree } => Self::persist_tree_to_path(&tree, path),
      AppTree::Forest { forest, .. } => self.persist_forest(&forest),
    }
  }

  fn run_init_cmd(&self, name: &str) -> Result<(), PutainDeMerdeError> {
    let tree = Tree::new(name, PROJECT_ICON);

    // if we have passed a specific tree path, create it at the given path and return
    match self.cli.path {
      Some(ref tree_path) => {
        return Self::persist_tree_to_path(&tree, tree_path);
      }

      _ => (),
    }

    let cwd = current_dir().map_err(PutainDeMerdeError::NoCWD)?;

    if self.cli.local {
      let path = Self::local_mind_path(cwd);
      return Self::persist_tree_to_path(&tree, path);
    }

    // check if we are in CWD
    if self.cli.cwd {
      // we need the forest first
      let mut forest = self.load_forest()?;
      forest.add_cwd_tree(cwd, tree);
      return self.persist_forest(&forest);
    }

    // create the main tree / forest
    match self.load_forest() {
      Ok(_) => Err(PutainDeMerdeError::AlreadyExists),

      // if this is the first time we create any tree, it’s logical we don’t have anything persisted yet; use
      // a default one
      Err(PutainDeMerdeError::NoForestPersisted) => {
        let forest = Forest::new(tree);
        self.persist_forest(&forest)
      }

      // any other error surfaces as error
      Err(e) => Err(e),
    }
  }

  fn run_insert_cmd(
    &self,
    mode: InsertMode,
    sel: Option<&str>,
    name: Option<&str>,
  ) -> Result<(), PutainDeMerdeError> {
    let tree = self.get_tree()?;
    let sel = self
      .ui
      .get_base_sel(
        ui::PickerOptions::either(self.cli.interactive, "Insert in: "),
        sel,
        NodeFilter::default(),
        &tree,
      )
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    let name = match name {
      Some(name) => Cow::from(name),
      None => Cow::from(self.ui.get_input_string("New name: ")?),
    };

    let node = Node::new(name.trim(), "");
    if node.name().is_empty() {
      return Err(PutainDeMerdeError::EmptyName);
    }

    match mode {
      InsertMode::InsideTop => sel.insert_top(node),
      InsertMode::InsideBottom => sel.insert_bottom(node),
      InsertMode::Before => sel.insert_before(node)?,
      InsertMode::After => sel.insert_after(node)?,
    }

    self.persist(tree)
  }

  fn run_remove_cmd(&self, sel: Option<&str>) -> Result<(), PutainDeMerdeError> {
    let tree = self.get_tree()?;
    let sel = self
      .ui
      .get_base_sel(
        ui::PickerOptions::either(self.cli.interactive, "Remove: "),
        sel,
        NodeFilter::default(),
        &tree,
      )
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    let parent = sel.parent()?;
    parent.delete(sel)?;

    self.persist(tree)
  }

  fn run_rename_cmd(
    &self,
    sel: Option<&str>,
    name: Option<&str>,
  ) -> Result<(), PutainDeMerdeError> {
    let tree = self.get_tree()?;
    let sel = self
      .ui
      .get_base_sel(
        ui::PickerOptions::either(self.cli.interactive, "Rename: "),
        sel,
        NodeFilter::default(),
        &tree,
      )
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    let name = match name {
      Some(name) => Cow::from(name),
      None => Cow::from(self.ui.get_input_string("New node name: ")?),
    };

    let name = name.trim();
    if name.is_empty() {
      return Err(PutainDeMerdeError::EmptyName);
    }

    sel.set_name(name)?;

    self.persist(tree)
  }

  fn run_icon_cmd(&self, sel: Option<&str>, icon: Option<&str>) -> Result<(), PutainDeMerdeError> {
    let tree = self.get_tree()?;
    let sel = self
      .ui
      .get_base_sel(
        ui::PickerOptions::either(self.cli.interactive, "Change icon: "),
        sel,
        NodeFilter::default(),
        &tree,
      )
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    let icon = match icon {
      Some(icon) => Cow::from(icon),
      None => Cow::from(self.ui.get_input_string("Change node icon > ")?),
    };

    sel.set_icon(icon.trim());
    self.persist(tree)
  }

  fn run_move_cmd(
    &self,
    mode: InsertMode,
    sel: Option<&str>,
    dest: Option<&str>,
  ) -> Result<(), PutainDeMerdeError> {
    let tree = self.get_tree()?;
    let sel = self
      .ui
      .get_base_sel(
        ui::PickerOptions::either(self.cli.interactive, "Source node: "),
        sel,
        NodeFilter::default(),
        &tree,
      )
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    let dest = self
      .ui
      .get_base_sel(
        ui::PickerOptions::either(self.cli.interactive, "Destination node: "),
        dest,
        NodeFilter::default(),
        &tree,
      )
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    match mode {
      InsertMode::InsideTop => dest.move_top(sel)?,
      InsertMode::InsideBottom => dest.move_bottom(sel)?,
      InsertMode::Before => dest.move_before(sel)?,
      InsertMode::After => dest.move_after(sel)?,
    }

    self.persist(tree)
  }

  fn run_paths_cmd(
    &self,
    sel: Option<&str>,
    ty: Option<DataType>,
  ) -> Result<(), PutainDeMerdeError> {
    let tree = self.get_tree()?;
    let prefix = sel.as_deref().unwrap_or("/");
    let filter = ty.map(DataType::to_filter).unwrap_or_default();

    let sel = self
      .ui
      .get_base_sel(
        ui::PickerOptions::either(self.cli.interactive, "Get paths: "),
        sel,
        NodeFilter::default(),
        &tree,
      )
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    sel.write_paths(prefix, filter, &mut io::stdout())?;
    self.persist(tree)
  }

  fn run_data_cmd(
    &self,
    sel: Option<&str>,
    ty: Option<DataType>,
    open: bool,
    cmd: &DataCommand,
  ) -> Result<(), PutainDeMerdeError> {
    let tree = self.get_tree()?;
    let filter = ty.map(DataType::to_filter).unwrap_or_default();
    let sel = self
      .ui
      .get_base_sel(
        ui::PickerOptions::either(self.cli.interactive, "Data node: "),
        sel,
        filter,
        &tree,
      )
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    match cmd {
      DataCommand::Get => {
        if let Some(content) = sel.data() {
          match (ty, content) {
            (None | Some(DataType::File), NodeData::File(path)) => {
              if open {
                self.ui.open_with_editor(path)?;
              } else {
                println!("{}", path.display())
              }
            }

            (None | Some(DataType::Link), NodeData::Link(link)) => {
              if open {
                self.ui.open_uri(link)?;
              } else {
                println!("{}", link);
              }
            }

            _ => Err(NodeError::MismatchDataType)?,
          }
        }

        Ok(())
      }

      DataCommand::Set { content } => {
        let data = match ty {
          Some(DataType::File) => NodeData::file(content),
          Some(DataType::Link) => NodeData::link(content),
          None => {
            if content.is_empty() {
              // TODO: support automatically setting the content based on the name and a template thing
              let path = self.data_file_store.create_data_file(
                sel.name(),
                self.config.edit.extension.as_deref().unwrap_or(".md"),
                "",
              )?;
              NodeData::file(path)
            } else {
              NodeData::link(content)
            }
          }
        };

        sel.set_data(data)?;
        self.persist(tree)
      }
    }
  }
}

fn main() {
  App::bootstrap();
}

#[derive(Debug, Error)]
pub enum PutainDeMerdeError {
  #[error("missing a base node selection")]
  MissingBaseSelection,

  #[error("no data directory available")]
  NoDataDir,

  #[error("the tree or forest already exists")]
  AlreadyExists,

  #[error("forbidden node operation")]
  NodeOperation(#[from] NodeError),

  #[error("no forest path; are you running without a filesystem?")]
  NoForestPath,

  #[error("no forest persisted yet")]
  NoForestPersisted,

  #[error("no tree persisted yet")]
  NoTreePersisted,

  #[error("error while serializing forest")]
  CannotSerializeForest(serde_json::Error),

  #[error("error while deserializing forest")]
  CannotDeserializeForest(serde_json::Error),

  #[error("error while creating directories on the filesystem")]
  CannotCreateDirectories(std::io::Error),

  #[error("error while writing forest to the filesystem")]
  CannotWriteForest(std::io::Error),

  #[error("error while reading forest from the filesystem")]
  CannotReadForest(std::io::Error),

  #[error("error while serializing specific tree from the filesystem")]
  CannotSerializeTree(serde_json::Error),

  #[error("error while deserializing specific tree from the filesystem")]
  CannotDeserializeTree(serde_json::Error),

  #[error("error while reading specific tree from the filesystem")]
  CannotReadTree(std::io::Error),

  #[error("error while writing specific tree from the filesystem")]
  CannotWriteTree(std::io::Error),

  #[error("no current working directory")]
  NoCWD(std::io::Error),

  #[error("no such CWD-based tree")]
  NoCWDTree(PathBuf),

  #[error("node with empty name")]
  EmptyName,

  #[error("cannot write a path")]
  CannotWritePath(io::Error),

  #[error("UI error: {0}")]
  UIError(#[from] UIError),

  #[error("data file store error: {0}")]
  DataFileStoreError(#[from] DataFileStoreError),
}

/// Application tree.
#[derive(Debug)]
enum AppTree {
  /// The tree lives on its own.
  Specific { path: PathBuf, tree: Tree },

  /// The tree lives in the forest.
  Forest { forest: Forest, tree: Tree },
}

impl Deref for AppTree {
  type Target = Tree;

  fn deref(&self) -> &Self::Target {
    match self {
      AppTree::Specific { tree, .. } | AppTree::Forest { tree, .. } => tree,
    }
  }
}
