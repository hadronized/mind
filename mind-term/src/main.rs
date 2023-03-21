mod cli;
mod config;
mod data_file;
mod ui;

use clap::Parser;
use cli::{Cli, Command, CommonArgs, DataType, InsertMode};
use colored::Colorize;
use config::Config;
use data_file::{DataFileStore, DataFileStoreError};
use mind_tree::forest::Forest;
use mind_tree::node::{path_iter, Node, NodeData, NodeError, NodeFilter};
use mind_tree::{encoding, node::Tree};
use std::borrow::Cow;
use std::env::current_dir;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::{fs, io};
use thiserror::Error;
use ui::{UIError, UI};

const PROJECT_ICON: &str = " ";

/// The top-level type holding everything that the application is about.
struct App {
  config: Config,
  cli: Cli,
  ui: UI,
  data_file_store: DataFileStore,
}

impl App {
  fn new() -> Result<Self, PutainDeMerdeError> {
    let config = Self::load_config();
    let cli = Cli::parse();
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
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent).map_err(PutainDeMerdeError::CannotCreateDirectories)?;
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
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent).map_err(PutainDeMerdeError::CannotCreateDirectories)?;
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
    if let Err(err) = Self::new().and_then(Self::run) {
      eprintln!("{}", err.to_string().red());
    }
  }

  /// Start and dispatch the application by looking at the CLI, config, etc.
  fn run(self) -> Result<(), PutainDeMerdeError> {
    match &self.cli.cmd {
      Command::Init { common_args, name } => self.run_init_cmd(common_args, name.as_deref()),

      Command::Insert {
        common_args,
        mode,
        source,
        name,
      } => self.run_insert_cmd(common_args, *mode, source.as_deref(), name.as_deref()),

      Command::Remove {
        common_args,
        source,
      } => self.run_remove_cmd(common_args, source.as_deref()),

      Command::Rename {
        common_args,
        source,
        new,
      } => self.run_rename_cmd(common_args, source.as_deref(), new.as_deref()),

      Command::Icon {
        common_args,
        source,
        icon,
      } => self.run_icon_cmd(common_args, source.as_deref(), icon.as_deref()),

      Command::Move {
        common_args,
        mode,
        source,
        dest,
      } => self.run_move_cmd(common_args, *mode, source.as_deref(), dest.as_deref()),

      Command::Paths {
        common_args,
        ty,
        source,
      } => self.run_paths_cmd(common_args, *ty, source.as_deref()),

      Command::Get {
        common_args,
        file,
        uri,
        open,
        source,
      } => self.run_get_cmd(common_args, *file, *uri, *open, source.as_deref()),

      Command::Set {
        common_args,
        file,
        uri,
        open,
        source,
      } => self.run_set_cmd(common_args, *file, uri.as_deref(), *open, source.as_deref()),
    }
  }

  fn get_tree(&self, common_args: &CommonArgs) -> Result<AppTree, PutainDeMerdeError> {
    match common_args.path {
      Some(ref tree_path) => Self::load_tree(tree_path).map(|tree| AppTree::Specific {
        path: tree_path.to_owned(),
        tree,
      }),

      None => {
        let cwd = current_dir().map_err(PutainDeMerdeError::NoCWD)?;

        if common_args.local {
          let path = Self::local_mind_path(&cwd);
          Self::load_tree(&path).map(|tree| AppTree::Specific { path, tree })
        } else if common_args.cwd {
          let forest = self.load_forest()?;
          forest
            .cwd_tree(cwd.clone())
            .cloned()
            .map(|tree| AppTree::Forest { forest, tree })
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
  fn persist(&self, tree: &AppTree) -> Result<(), PutainDeMerdeError> {
    match tree {
      AppTree::Specific { path, tree } => Self::persist_tree_to_path(tree, path),
      AppTree::Forest { forest, .. } => self.persist_forest(forest),
    }
  }

  fn run_init_cmd(
    &self,
    common_args: &CommonArgs,
    name: Option<&str>,
  ) -> Result<(), PutainDeMerdeError> {
    let name = name
      .map(Cow::from)
      .or_else(|| {
        self
          .ui
          .input(ui::PickerOptions::either(
            common_args.interactive,
            "Mind tree name: ",
          ))
          .map(Cow::from)
      })
      .ok_or(PutainDeMerdeError::EmptyName)?;
    let tree = Tree::new(name, PROJECT_ICON);

    // if we have passed a specific tree path, create it at the given path and return
    if let Some(ref tree_path) = common_args.path {
      return Self::persist_tree_to_path(&tree, tree_path);
    }

    let cwd = current_dir().map_err(PutainDeMerdeError::NoCWD)?;

    if common_args.local {
      let path = Self::local_mind_path(cwd);
      return Self::persist_tree_to_path(&tree, path);
    }

    // check if we are in CWD
    if common_args.cwd {
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
    common_args: &CommonArgs,
    mode: InsertMode,
    source: Option<&str>,
    name: Option<&str>,
  ) -> Result<(), PutainDeMerdeError> {
    let tree = self.get_tree(common_args)?;

    let source = source
      .map(Cow::from)
      .or_else(|| {
        self
          .ui
          .select_path(
            ui::PickerOptions::either(common_args.interactive, "Insert in: "),
            NodeFilter::default(),
            &tree,
          )
          .map(Cow::from)
      })
      .and_then(|path| tree.get_node_by_path(path_iter(&path), self.config.tree.auto_create_nodes))
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    let name = name
      .map(Cow::from)
      .or_else(|| {
        self
          .ui
          .input(ui::PickerOptions::either(
            common_args.interactive,
            "New name: ",
          ))
          .map(Cow::from)
      })
      .ok_or(PutainDeMerdeError::EmptyName)?;
    let name = name.trim();
    if name.is_empty() {
      return Err(PutainDeMerdeError::EmptyName);
    }

    let node = Node::new(name.trim(), "");
    match mode {
      InsertMode::InsideTop => source.insert_top(node),
      InsertMode::InsideBottom => source.insert_bottom(node),
      InsertMode::Before => source.insert_before(node)?,
      InsertMode::After => source.insert_after(node)?,
    }

    self.persist(&tree)
  }

  fn run_remove_cmd(
    &self,
    common_args: &CommonArgs,
    source: Option<&str>,
  ) -> Result<(), PutainDeMerdeError> {
    let tree = self.get_tree(common_args)?;

    let source = source
      .map(Cow::from)
      .or_else(|| {
        self
          .ui
          .select_path(
            ui::PickerOptions::either(common_args.interactive, "Remove: "),
            NodeFilter::default(),
            &tree,
          )
          .map(Cow::from)
      })
      .and_then(|path| tree.get_node_by_path(path_iter(&path), false))
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    let parent = source.parent()?;
    parent.delete(source)?;

    self.persist(&tree)
  }

  fn run_rename_cmd(
    &self,
    common_args: &CommonArgs,
    source: Option<&str>,
    new: Option<&str>,
  ) -> Result<(), PutainDeMerdeError> {
    let tree = self.get_tree(common_args)?;

    let source = source
      .map(Cow::from)
      .or_else(|| {
        self
          .ui
          .select_path(
            ui::PickerOptions::either(common_args.interactive, "Rename: "),
            NodeFilter::default(),
            &tree,
          )
          .map(Cow::from)
      })
      .and_then(|path| tree.get_node_by_path(path_iter(&path), false))
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    let name = new
      .map(Cow::from)
      .or_else(|| {
        self
          .ui
          .input(ui::PickerOptions::either(
            common_args.interactive,
            "New node name:",
          ))
          .map(Cow::from)
      })
      .ok_or(PutainDeMerdeError::EmptyName)?;

    let name = name.trim();
    if name.is_empty() {
      return Err(PutainDeMerdeError::EmptyName);
    }

    source.set_name(name)?;
    self.persist(&tree)
  }

  fn run_icon_cmd(
    &self,
    common_args: &CommonArgs,
    source: Option<&str>,
    icon: Option<&str>,
  ) -> Result<(), PutainDeMerdeError> {
    let tree = self.get_tree(common_args)?;
    let source = source
      .map(Cow::from)
      .or_else(|| {
        self
          .ui
          .select_path(
            ui::PickerOptions::either(common_args.interactive, "Change icon: "),
            NodeFilter::default(),
            &tree,
          )
          .map(Cow::from)
      })
      .and_then(|path| tree.get_node_by_path(path_iter(&path), self.config.tree.auto_create_nodes))
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    let icon = icon
      .map(Cow::from)
      .or_else(|| {
        self
          .ui
          .input(ui::PickerOptions::either(
            common_args.interactive,
            "Change node icon: ",
          ))
          .map(Cow::from)
      })
      .unwrap_or_else(|| Cow::from(""));

    source.set_icon(icon.trim());
    self.persist(&tree)
  }

  fn run_move_cmd(
    &self,
    common_args: &CommonArgs,
    mode: InsertMode,
    source: Option<&str>,
    dest: Option<&str>,
  ) -> Result<(), PutainDeMerdeError> {
    let tree = self.get_tree(common_args)?;

    let source = source
      .map(Cow::from)
      .or_else(|| {
        self
          .ui
          .select_path(
            ui::PickerOptions::either(common_args.interactive, "Source node: "),
            NodeFilter::default(),
            &tree,
          )
          .map(Cow::from)
      })
      .and_then(|path| tree.get_node_by_path(path_iter(&path), self.config.tree.auto_create_nodes))
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    let dest = dest
      .map(Cow::from)
      .or_else(|| {
        self
          .ui
          .select_path(
            ui::PickerOptions::either(common_args.interactive, "Destination node: "),
            NodeFilter::default(),
            &tree,
          )
          .map(Cow::from)
      })
      .and_then(|path| tree.get_node_by_path(path_iter(&path), self.config.tree.auto_create_nodes))
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    match mode {
      InsertMode::InsideTop => dest.move_top(source)?,
      InsertMode::InsideBottom => dest.move_bottom(source)?,
      InsertMode::Before => dest.move_before(source)?,
      InsertMode::After => dest.move_after(source)?,
    }

    self.persist(&tree)
  }

  fn run_paths_cmd(
    &self,
    common_args: &CommonArgs,
    ty: Option<DataType>,
    source: Option<&str>,
  ) -> Result<(), PutainDeMerdeError> {
    let tree = self.get_tree(common_args)?;
    let prefix = source.unwrap_or("/");
    let filter = ty.map(DataType::to_filter).unwrap_or_default();

    let source = source
      .map(Cow::from)
      .or_else(|| {
        self
          .ui
          .select_path(
            ui::PickerOptions::either(common_args.interactive, "Get paths: "),
            filter,
            &tree,
          )
          .map(Cow::from)
      })
      .and_then(|path| tree.get_node_by_path(path_iter(&path), self.config.tree.auto_create_nodes))
      .unwrap_or_else(|| tree.root());

    source.write_paths(prefix, filter, &mut io::stdout())?;

    self.persist(&tree)
  }

  fn run_get_cmd(
    &self,
    common_args: &CommonArgs,
    file: bool,
    uri: bool,
    open: bool,
    source: Option<&str>,
  ) -> Result<(), PutainDeMerdeError> {
    let tree = self.get_tree(common_args)?;
    let filter = NodeFilter::new(file, uri);

    let source = source
      .map(Cow::from)
      .or_else(|| {
        self
          .ui
          .select_path(
            ui::PickerOptions::either(common_args.interactive, "Get data of: "),
            filter,
            &tree,
          )
          .map(Cow::from)
      })
      .and_then(|path| tree.get_node_by_path(path_iter(&path), self.config.tree.auto_create_nodes))
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    self.get_open_data(open, &source)
  }

  fn run_set_cmd(
    &self,
    common_args: &CommonArgs,
    file: bool,
    uri: Option<&str>,
    open: bool,
    source: Option<&str>,
  ) -> Result<(), PutainDeMerdeError> {
    let tree = self.get_tree(common_args)?;

    let source = source
      .map(Cow::from)
      .or_else(|| {
        self
          .ui
          .select_path(
            ui::PickerOptions::either(common_args.interactive, "Set data for: "),
            NodeFilter::default(),
            &tree,
          )
          .map(Cow::from)
      })
      .and_then(|path| tree.get_node_by_path(path_iter(&path), self.config.tree.auto_create_nodes))
      .ok_or(PutainDeMerdeError::MissingBaseSelection)?;

    match (file, uri) {
      (true, None) => self.check_create_open_data(open, &source, "")?,
      (true, Some(_)) => return Err(PutainDeMerdeError::CannotSetURIAndfileData),
      (false, None) => return Err(PutainDeMerdeError::NodeOperation(NodeError::NoData)),
      (false, Some(uri)) => {
        let uri = if uri.is_empty() {
          self
            .ui
            .input(ui::PickerOptions::either(common_args.interactive, "URI: "))
            .map(Cow::from)
            .ok_or(PutainDeMerdeError::EmptyURI)?
        } else {
          Cow::from(uri)
        };
        self.check_create_open_data(open, &source, &uri)?
      }
    }

    self.persist(&tree)
  }

  /// Check whether we need to create and associate data, and eventually open the associated data.
  fn check_create_open_data(
    &self,
    open: bool,
    node: &Node,
    data: &str,
  ) -> Result<(), PutainDeMerdeError> {
    if let Some(NodeData::File(_)) = node.data() {
      return Err(PutainDeMerdeError::DataAlreadyExists);
    }

    let data = if data.is_empty() {
      // TODO: support automatically setting the content based on the name and a template thing
      let path = self.data_file_store.create_data_file(
        node.name(),
        self.config.ui.extension.as_deref().unwrap_or(".md"),
        "",
      )?;
      NodeData::file(path)
    } else {
      NodeData::link(data)
    };

    // move inside above
    node.set_data(data)?;

    if open {
      self.get_open_data(open, node)?;
    }

    Ok(())
  }

  /// Get or open the data associated with a node.
  fn get_open_data(&self, open: bool, node: &Node) -> Result<(), PutainDeMerdeError> {
    if let Some(content) = node.data() {
      match content {
        NodeData::File(path) => {
          if open {
            self.ui.open_with_editor(path)?;
          } else {
            println!("{}", path.display())
          }
        }

        NodeData::Link(link) => {
          if open {
            self.ui.open_uri(link)?;
          } else {
            println!("{}", link);
          }
        }
      }
    }

    Ok(())
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

  #[error("node error: {0}")]
  NodeOperation(#[from] NodeError),

  #[error("no forest path; are you running without a filesystem?")]
  NoForestPath,

  #[error("no forest persisted yet")]
  NoForestPersisted,

  #[error("no tree persisted yet")]
  NoTreePersisted,

  #[error("error while serializing forest: {0}")]
  CannotSerializeForest(serde_json::Error),

  #[error("error while deserializing forest: {0}")]
  CannotDeserializeForest(serde_json::Error),

  #[error("error while creating directories on the filesystem: {0}")]
  CannotCreateDirectories(std::io::Error),

  #[error("error while writing forest to the filesystem: {0}")]
  CannotWriteForest(std::io::Error),

  #[error("error while reading forest from the filesystem: {0}")]
  CannotReadForest(std::io::Error),

  #[error("error while serializing specific tree from the filesystem: {0}")]
  CannotSerializeTree(serde_json::Error),

  #[error("error while deserializing specific tree from the filesystem: {0}")]
  CannotDeserializeTree(serde_json::Error),

  #[error("error while reading specific tree from the filesystem: {0}")]
  CannotReadTree(std::io::Error),

  #[error("error while writing specific tree from the filesystem: {0}")]
  CannotWriteTree(std::io::Error),

  #[error("no current working directory: {0}")]
  NoCWD(std::io::Error),

  #[error("no such CWD-based tree: {0}")]
  NoCWDTree(PathBuf),

  #[error("node with empty name")]
  EmptyName,

  #[error("node with no URI")]
  EmptyURI,

  #[error("data already exists and cannot be replaced")]
  DataAlreadyExists,

  #[error("cannot set both URI and file data on a node")]
  CannotSetURIAndfileData,

  #[error("cannot write a path: {0}")]
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
