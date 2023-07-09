use std::path::PathBuf;

use clap::Parser;
use log::LevelFilter;

#[derive(Parser)]
pub struct Cli {
  #[clap(long)]
  pub log_file: Option<PathBuf>,

  #[clap(short, long, action = clap::ArgAction::Count)]
  pub verbose: u8,
}

impl Cli {
  pub fn verbosity(&self) -> LevelFilter {
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
