use std::{str::FromStr, sync::mpsc::Sender, time::Duration};

use tui::{
  style::{Color, Style},
  text::Span,
};

use crate::{
  components::{menu::MenuItem, tree::TuiTree},
  error::AppError,
  ops::InsertMode,
};

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

  /// Ask the TUI to create node data (prompt the user for the menu).
  PromptNodeData {
    // Sender to reply with.
    sender: Sender<Option<MenuItem>>,
  },
}

impl Request {
  /// Display a sticky message in the information window.
  pub fn sticky_msg(span: impl Into<Span<'static>>, timeout: Duration) -> Self {
    Self::StickyMsg {
      span: span.into(),
      timeout,
    }
  }

  /// Display an informational message in the information window.
  pub fn info_msg(msg: impl Into<String>) -> Self {
    let span = Span::styled(msg.into(), Style::default().fg(Color::Blue));
    Self::sticky_msg(span, Duration::from_secs(5))
  }

  /// Display a warning message in the information window.
  pub fn warn_msg(msg: impl Into<String>) -> Self {
    let span = Span::styled(msg.into(), Style::default().fg(Color::Yellow));
    Self::sticky_msg(span, Duration::from_secs(5))
  }

  /// Display an error message in the information window.
  pub fn err_msg(msg: impl Into<String>) -> Self {
    let span = Span::styled(msg.into(), Style::default().fg(Color::Red));
    Self::sticky_msg(span, Duration::from_secs(5))
  }
}

/// User commands.
///
/// Those commands can be sent by typing them in the command line, for now.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum UserCmd {
  /// The user wants to (force) quit.
  Quit { force: bool },

  /// The user wants to save the current tree.
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
