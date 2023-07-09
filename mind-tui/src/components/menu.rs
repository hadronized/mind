use std::sync::mpsc::Sender;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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

/// An option menu.
///
/// This menu will display a list of selections and will return the one the user selected, or [`None`] if the menu is
/// aborted.
#[derive(Debug, Default)]
pub struct Menu {
  items: Vec<MenuItem>,
  currently_selected: usize, // index in items
}

impl Menu {
  pub fn new(items: impl Into<Vec<MenuItem>>) -> Self {
    Self {
      items: items.into(),
      currently_selected: 0,
    }
  }

  pub fn select_prev(&mut self) {
    self.currently_selected = self.currently_selected.max(1) - 1;
  }

  pub fn select_next(&mut self) {
    self.currently_selected = (self.items.len() - 1).min(self.currently_selected + 1);
  }

  pub fn selected(&self) -> Option<&str> {
    self
      .items
      .get(self.currently_selected)
      .map(|item| item.name.as_str())
  }
}

impl RawEventHandler for Menu {
  type Feedback = Option<Option<MenuItem>>;

  fn react_raw(
    &mut self,
    event: crossterm::event::Event,
  ) -> Result<(HandledEvent, Self::Feedback), AppError> {
    match event {
      // select previous item
      crossterm::event::Event::Key(KeyEvent {
        code: KeyCode::Char('p'),
        modifiers: KeyModifiers::CONTROL,
        ..
      }) => {
        self.select_prev();
        Ok((HandledEvent::handled(), None))
      }

      // select next item
      crossterm::event::Event::Key(KeyEvent {
        code: KeyCode::Char('n'),
        modifiers: KeyModifiers::CONTROL,
        ..
      }) => {
        self.select_next();
        Ok((HandledEvent::handled(), None))
      }

      // return the selected item on return
      crossterm::event::Event::Key(KeyEvent {
        code: KeyCode::Enter,
        ..
      }) => {
        let item = self.items.get(self.currently_selected).cloned();
        Ok((HandledEvent::handled(), Some(item)))
      }

      // cancel
      crossterm::event::Event::Key(KeyEvent {
        code: KeyCode::Esc, ..
      }) => Ok((HandledEvent::handled(), Some(None))),

      // select item directly by pressing a key
      crossterm::event::Event::Key(KeyEvent {
        code: KeyCode::Char(c),
        ..
      }) => {
        if let Some((i, item)) = self
          .items
          .iter()
          .enumerate()
          .find(|(_, item)| item.key == Some(c))
        {
          self.currently_selected = i;
          return Ok((HandledEvent::handled(), Some(Some(item.clone()))));
        }

        Ok((HandledEvent::Unhandled(event), None))
      }

      _ => Ok((HandledEvent::Unhandled(event), None)),
    }
  }
}

impl<'a> Widget for &'a Menu {
  fn render(self, area: Rect, buf: &mut Buffer) {
    // center on area.width based on the longest item
    //let longest_width = self.items.iter().map(|item| item.name.len()).max();

    for (i, item) in self.items.iter().enumerate() {
      let mut x = area.x;

      // if there is a key set, print it first and shift x by the length
      if let Some(key) = item.key {
        let s = format!("({key}) ");
        buf.set_string(x, area.y + i as u16, &s, Style::default());
        x += s.len() as u16;
      }

      // then just render the actual menu item
      buf.set_string(x, area.y + i as u16, &item.name, Style::default());

      // highlight if selected
      if i == self.currently_selected {
        buf.set_style(
          Rect {
            height: 1,
            y: area.y + i as u16,
            ..area
          },
          Style::default()
            .bg(Color::Black)
            .add_modifier(Modifier::DIM),
        );
      }
    }
  }
}

/// A menu item.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MenuItem {
  name: String,
  key: Option<char>,
}

impl MenuItem {
  pub fn new(name: impl Into<String>, key: impl Into<Option<char>>) -> Self {
    Self {
      name: name.into(),
      key: key.into(),
    }
  }
}

impl<S> From<S> for MenuItem
where
  S: Into<String>,
{
  fn from(value: S) -> Self {
    MenuItem {
      name: value.into(),
      key: None,
    }
  }
}

#[derive(Debug, Default)]
pub struct TuiMenu {
  menu: Option<(Menu, Sender<Option<MenuItem>>)>,
}

impl TuiMenu {
  pub fn show(&mut self, items: impl Into<Vec<MenuItem>>, sender: Sender<Option<MenuItem>>) {
    let menu = Menu::new(items);
    self.menu = Some((menu, sender));
  }

  pub fn is_visible(&self) -> bool {
    self.menu.is_some()
  }

  pub fn height(&self) -> u16 {
    self
      .menu
      .as_ref()
      .map(|(menu, _)| menu.items.len())
      .unwrap_or_default() as _
  }
}

impl<'a> Widget for &'a TuiMenu {
  fn render(self, area: Rect, buf: &mut Buffer) {
    if let Some((ref menu, _)) = self.menu {
      menu.render(area, buf);
    }
  }
}

impl RawEventHandler for TuiMenu {
  type Feedback = ();

  fn react_raw(
    &mut self,
    event: crossterm::event::Event,
  ) -> Result<(HandledEvent, Self::Feedback), AppError> {
    if let Some((menu, sender)) = &mut self.menu {
      let (_, selected) = menu.react_raw(event)?;

      if let Some(selected) = selected {
        sender.send(selected).unwrap();
        self.menu = None;
      }

      Ok((HandledEvent::handled(), ()))
    } else {
      Ok((HandledEvent::Unhandled(event), ()))
    }
  }
}
