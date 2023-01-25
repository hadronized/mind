use std::path::PathBuf;

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub struct Config {
  /// Path to a Mind tree.
  #[structopt(short, long)]
  pub path: Option<PathBuf>,

  /// Select a base node to operate on.
  #[structopt(short = "s", long = "sel")]
  pub base_sel: Option<String>,

  #[structopt(subcommand)]
  pub cmd: Command,
}

#[derive(Debug, StructOpt)]
pub enum Command {}
