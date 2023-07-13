//! Filesystem features, such as creating data files for data nodes.

use chrono::{Datelike, Timelike};
use std::{fs, io, path::PathBuf};
use thiserror::Error;

/// Errors that might happen when dealing with data file stores.
#[derive(Debug, Error)]
pub enum DataFileStoreError {
  #[error("filesystem error: {0}")]
  FileSystemError(#[from] io::Error),
}

/// Store for creating data files.
#[derive(Debug)]
pub struct DataFileStore {
  root: PathBuf,
}

impl DataFileStore {
  pub fn new(root: impl Into<PathBuf>) -> Self {
    Self { root: root.into() }
  }

  /// Create a new data file with the (sanitized) input name.
  pub fn create_data_file(
    &self,
    name: impl AsRef<str>,
    ext: impl AsRef<str>,
    contents: impl AsRef<str>,
  ) -> Result<PathBuf, DataFileStoreError> {
    // sanitize the name first
    let sanitized = Self::sanitize_name(name.as_ref());

    let now = chrono::Utc::now();
    let name = format!(
      "{year}{month}{day}{hour}{minute}{second}-{name}{ext}",
      year = now.year(),
      month = now.month(),
      day = now.day(),
      hour = now.hour(),
      minute = now.minute(),
      second = now.second(),
      name = sanitized,
      ext = ext.as_ref()
    );
    let path = self.root.join(name);

    fs::create_dir_all(&self.root)?;
    fs::write(&path, contents.as_ref())?;
    Ok(path)
  }

  fn sanitize_name(name: &str) -> String {
    name
      .trim()
      .chars()
      .filter_map(|c| {
        if [' ', '.', '/', '\\'].contains(&c) {
          Some('-')
        } else if c.is_ascii_alphanumeric() || ['-', '_'].contains(&c) {
          Some(c)
        } else {
          None
        }
      })
      .collect()
  }
}
