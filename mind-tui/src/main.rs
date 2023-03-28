use crossterm::{
  execute,
  terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{thread, time::Duration};
use thiserror::Error;
use tui::{
  backend::CrosstermBackend,
  layout::{Constraint, Layout},
  style::{Color, Style},
  widgets::{Block, Borders, List, ListItem},
  Terminal,
};

#[derive(Debug, Error)]
enum AppError {
  #[error("initialization failed: {0}")]
  Init(std::io::Error),

  #[error("termination failed: {0}")]
  Termination(std::io::Error),

  #[error("terminal action failed: {0}")]
  TerminalAction(crossterm::ErrorKind),

  #[error("render error: {0}")]
  Render(std::io::Error),
}

fn main() {
  if let Err(err) = bootstrap() {
    eprintln!("{}", err);
    std::process::exit(1);
  }
}

fn bootstrap() -> Result<(), AppError> {
  enable_raw_mode().map_err(AppError::Init)?;
  let mut stdout = std::io::stdout();
  execute!(&mut stdout, EnterAlternateScreen).map_err(AppError::TerminalAction)?;
  let mut terminal = Terminal::new(CrosstermBackend::new(stdout)).map_err(AppError::Init)?;

  terminal.hide_cursor().map_err(AppError::TerminalAction)?;

  let frame_size = terminal.get_frame().size();
  let rects = Layout::default()
    .constraints([
      Constraint::Percentage(25),
      Constraint::Percentage(50),
      Constraint::Percentage(25),
    ])
    .direction(tui::layout::Direction::Vertical)
    .split(frame_size);

  terminal
    .current_buffer_mut()
    .set_style(rects[0], Style::default().fg(Color::Blue));
  terminal
    .draw(|f| {
      let top = Block::default().title("Top").borders(Borders::ALL);

      let bottom = Block::default().title("Bottom").borders(Borders::ALL);
      f.render_widget(top, rects[0]);
      f.render_widget(bottom, rects[2]);

      let items = List::new([ListItem::new("Something"), ListItem::new("Something else")]);
      f.render_widget(items, rects[1]);
    })
    .map_err(AppError::Render)?;

  thread::sleep(Duration::from_secs(5));
  terminal.show_cursor().map_err(AppError::TerminalAction)?;

  execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(AppError::TerminalAction)?;
  disable_raw_mode().map_err(AppError::Termination)?;

  Ok(())
}
