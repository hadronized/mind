use crate::{error::AppError, ops::InsertMode, req::UserCmd};

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
  pub fn handled() -> Self {
    Self::Handled {
      requires_redraw: true,
    }
  }

  pub fn and_then<T>(self, next_handler: &mut T) -> Result<HandledEvent, AppError>
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

/// Event emitted in the TUI when something happens.
#[derive(Clone, Debug)]
pub enum Event {
  /// A command was entered.
  Command(UserCmd),

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

  /// Node data open.
  OpenNodeData { id: usize },

  /// Rename a node.
  RenameNode { id: usize, rename: String },

  /// Node marked.
  MarkedNode { id: Option<usize> },
}

impl Event {
  /// Update the event with a string input.
  ///
  /// Some events can be _pending_ with no string. Once the TUI has gathered the input prompt, the event is provided
  /// with the input from the user and then emitted.
  ///
  /// It’s also possible to use the input to « validate » the event to be emitted, such as confirmation popups.
  pub fn accept_input(self, input: String) -> Option<Self> {
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

      Event::RenameNode { id, .. } => Some(Event::RenameNode { id, rename: input }),

      _ => None,
    }
  }
}
