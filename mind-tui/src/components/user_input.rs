use std::sync::mpsc::Sender;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use tui::{
  buffer::Buffer,
  layout::Rect,
  style::{Color, Modifier, Style},
  widgets::Widget,
};

use crate::{
  error::AppError,
  event::{HandledEvent, RawEventHandler},
};

/// Component displaying an input prompt to ask data from the user.
#[derive(Debug, Default)]
pub struct UserInputPrompt {
  prompt: Option<(InputPrompt, Sender<Option<String>>)>,
  completions: Vec<String>,
}

impl UserInputPrompt {
  pub fn new_with_completions(completions: impl Into<Vec<String>>) -> Self {
    Self {
      prompt: None,
      completions: completions.into(),
    }
  }

  pub fn is_visible(&self) -> bool {
    self.prompt.is_some()
  }

  pub fn show(&mut self, sender: Sender<Option<String>>) {
    let prompt = InputPrompt {
      completions: self.completions.clone(),
      ..InputPrompt::default()
    };

    self.prompt = Some((prompt, sender));
  }

  pub fn show_with_title(&mut self, title: impl Into<String>, sender: Sender<Option<String>>) {
    let prompt = InputPrompt {
      completions: self.completions.clone(),
      title: title.into(),
      ..InputPrompt::default()
    };
    self.prompt = Some((prompt, sender));
  }

  pub fn prompt(&self) -> Option<&InputPrompt> {
    self.prompt.as_ref().map(|(prompt, _)| prompt)
  }
}

impl RawEventHandler for UserInputPrompt {
  type Feedback = ();

  fn react_raw(
    &mut self,
    event: crossterm::event::Event,
  ) -> Result<(HandledEvent, Self::Feedback), AppError> {
    if let Some((ref mut input_prompt, _)) = self.prompt {
      if let crossterm::event::Event::Key(KeyEvent {
        code,
        kind: KeyEventKind::Press,
        ..
      }) = event
      {
        match code {
          KeyCode::Esc => {
            if let Some((_, sender)) = self.prompt.take() {
              sender.send(None).unwrap();
            }

            return Ok((HandledEvent::handled(), ()));
          }

          KeyCode::Enter => {
            // command line is complete
            if let Some((prompt, sender)) = self.prompt.take() {
              let input = prompt.as_str().to_owned();
              sender.send(Some(input)).unwrap();
            }

            return Ok((HandledEvent::handled(), ()));
          }

          KeyCode::Char(c) => {
            input_prompt.push_char(c);
            return Ok((HandledEvent::handled(), ()));
          }

          KeyCode::Backspace => {
            input_prompt.pop_char();
            return Ok((HandledEvent::handled(), ()));
          }

          KeyCode::Left => {
            input_prompt.move_cursor_left();
            return Ok((HandledEvent::handled(), ()));
          }

          KeyCode::Right => {
            input_prompt.move_cursor_right();
            return Ok((HandledEvent::handled(), ()));
          }

          _ => (),
        }
      }
    }

    Ok((HandledEvent::Unhandled(event), ()))
  }
}

#[derive(Debug)]
pub struct InputPrompt {
  input: String,
  cursor: usize,
  title: String,
  completions: Vec<String>,
}

impl Default for InputPrompt {
  fn default() -> Self {
    Self {
      input: String::default(),
      cursor: 0,
      title: ":".to_owned(),
      completions: Vec::new(),
    }
  }
}

impl InputPrompt {
  fn to_byte_pos(&self, cursor: usize) -> usize {
    self.input.chars().take(cursor).map(char::len_utf8).sum()
  }

  fn push_char(&mut self, c: char) {
    let index = self.to_byte_pos(self.cursor);
    self.input.insert(index, c);
    self.move_cursor_right();
  }

  fn pop_char(&mut self) -> Option<char> {
    if self.input.is_empty() || self.cursor == 0 {
      None
    } else {
      let index = self.to_byte_pos(self.cursor - 1);
      let char = self.input.remove(index);
      self.move_cursor_left();
      Some(char)
    }
  }

  fn move_cursor_left(&mut self) {
    self.cursor = self.cursor.saturating_sub(1);
  }

  fn move_cursor_right(&mut self) {
    self.cursor = self.cursor.min(self.input.len() - 1) + 1;
  }

  pub fn cursor(&self) -> usize {
    self.cursor
  }

  fn as_str(&self) -> &str {
    self.input.as_str()
  }
}

impl<'a> Widget for &'a InputPrompt {
  fn render(self, area: Rect, buf: &mut Buffer) {
    // render the prefix grey with no text; green if the function is valid and red if not
    let input_str = self.input.as_str();
    let color = if self.completions.is_empty() {
      Color::Blue
    } else if self.completions.iter().any(|c| c == input_str) {
      Color::Green
    } else {
      Color::Red
    };

    buf.set_string(area.x, area.y, &self.title, Style::default().fg(color));
    buf.set_string(
      area.x + self.title.len() as u16,
      area.y,
      &self.input,
      Style::default()
        .fg(Color::Magenta)
        .remove_modifier(Modifier::all()),
    );
  }
}
