use std::{path::PathBuf, sync::mpsc::SendError};

use mind_tree::{data_file::DataFileStoreError, forest::ForestError, node::NodeError};
use thiserror::Error;

use crate::req::Request;

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
  Request(#[from] SendError<Request>),

  #[error("rendering error: {0}")]
  Render(std::io::Error),

  #[error("unknown '{0}' command")]
  UnknownCommand(String),

  #[error("no data directory available")]
  NoDataDir,

  #[error("no forest path")]
  NoForestPath,

  #[error("forest error: {0}")]
  ForestError(#[from] ForestError),

  #[error("node error: {0}")]
  NodeError(#[from] NodeError),

  #[error("cannot open URL {url}: {err}")]
  URLOpenError { url: String, err: String },

  #[error("cannot open node path {path}: {err}")]
  NodePathOpenError { path: PathBuf, err: String },

  #[error("cannot configure editor: {err}")]
  EditorConfig { err: String },

  #[error("error while creating data file: {err}")]
  DataFileStoreError {
    #[from]
    err: DataFileStoreError,
  },
}
