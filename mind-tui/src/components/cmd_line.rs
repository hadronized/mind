use std::sync::mpsc::{channel, Receiver, Sender};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

use crate::{
  error::AppError,
  event::{Event, HandledEvent, RawEventHandler},
};

use super::user_input::{InputPrompt, UserInputPrompt};

#[derive(Debug)]
pub struct CmdLine {
  input_prompt: UserInputPrompt,
  prompt_rx: Receiver<Option<String>>,
  prompt_sx: Sender<Option<String>>,
  event_sx: Sender<Event>,
}

impl CmdLine {
  pub fn new(event_sx: Sender<Event>) -> Self {
    let input_prompt = UserInputPrompt::new_with_completions([
      "q".to_owned(),
      "q!".to_owned(),
      "quit".to_owned(),
      "quit!".to_owned(),
      "w".to_owned(),
      "write".to_owned(),
    ]);

    let (prompt_sx, prompt_rx) = channel();
    Self {
      input_prompt,
      prompt_rx,
      prompt_sx,
      event_sx,
    }
  }

  pub fn prompt(&self) -> Option<&InputPrompt> {
    self.input_prompt.prompt()
  }
}

impl RawEventHandler for CmdLine {
  type Feedback = ();

  fn react_raw(
    &mut self,
    event: crossterm::event::Event,
  ) -> Result<(HandledEvent, Self::Feedback), AppError> {
    if self.input_prompt.is_visible() {
      let (handled, _) = self.input_prompt.react_raw(event)?;

      if let Ok(Some(input)) = self.prompt_rx.try_recv() {
        // command line is complete
        let usr_cmd = input.parse()?;

        self
          .event_sx
          .send(Event::Command(usr_cmd))
          .map_err(|e| AppError::Event(e.to_string()))?;
      }

      return Ok((handled, ()));
    } else if let crossterm::event::Event::Key(KeyEvent {
      code: KeyCode::Char(':'),
      kind: KeyEventKind::Press,
      ..
    }) = event
    {
      self.input_prompt.show(self.prompt_sx.clone());
      return Ok((HandledEvent::handled(), ()));
    }

    Ok((HandledEvent::Unhandled(event), ()))
  }
}
