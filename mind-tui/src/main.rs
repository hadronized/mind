use crossterm::{
  event::{
    EnableMouseCapture, KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind,
  },
  execute,
  terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
  io::Stdout,
  process::exit,
  sync::mpsc::{channel, Receiver, Sender},
  thread,
  time::{Duration, Instant},
};
use thiserror::Error;
use tui::{backend::CrosstermBackend, Terminal};

fn main() {
  if let Err(err) = bootstrap() {
    eprintln!("{}", err);
    exit(1);
  }
}

fn bootstrap() -> Result<(), AppError> {
  let (event_sx, event_rx) = channel();
  let (request_sx, request_rx) = channel();

  // spawn a thread for the TUI; we can send requests to it and it sends events back to us
  let tui_thread = thread::spawn(move || {
    let tui = Tui::new(event_sx, request_rx).expect("TUI creation");
    if let Err(err) = tui.run() {
      eprintln!("TUI exited with error: {}", err);
      exit(1);
    }
  });

  // main loop of our logic application
  while let Ok(event) = event_rx.recv() {
    match event {
      Event::KeyPressed(key) => {
        if let KeyCode::Char('q') = key {
          request_sx.send(Request::Quit).unwrap();
        }
      }

      _ => (),
    }
  }

  if let Err(err) = tui_thread.join() {
    eprintln!("TUI killed while waiting for it: {:?}", err);
    exit(1);
  }

  Ok(())
}

#[derive(Debug, Error)]
enum AppError {
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
}

/// Event emitted in the TUI when something happens.
#[derive(Clone, Debug)]
pub enum Event {
  /// A click was performed somewhere
  Click,

  /// A key was pressed.
  KeyPressed(KeyCode),
}

/// Request sent to the TUI to make a change in it.
#[derive(Clone, Debug)]
pub enum Request {
  /// Ask the TUI to quit.
  Quit,
}

struct Tui {
  terminal: Terminal<CrosstermBackend<Stdout>>,
  event_sx: Sender<Event>,
  request_rx: Receiver<Request>,
}

impl Tui {
  const CLICK_TIMING: Duration = Duration::from_millis(100);

  pub fn new(event_sx: Sender<Event>, request_rx: Receiver<Request>) -> Result<Self, AppError> {
    enable_raw_mode().map_err(AppError::Init)?;

    let mut stdout = std::io::stdout();
    execute!(&mut stdout, EnterAlternateScreen, EnableMouseCapture)
      .map_err(AppError::TerminalAction)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout)).map_err(AppError::Init)?;

    terminal.hide_cursor().map_err(AppError::TerminalAction)?;

    Ok(Tui {
      terminal,
      event_sx,
      request_rx,
    })
  }

  pub fn run(self) -> Result<(), AppError> {
    let mut left_button_down_at = None;

    loop {
      // dequeue events
      while let Ok(true) = crossterm::event::poll(Duration::from_millis(10)) {
        // event available
        let event = crossterm::event::read().map_err(AppError::TerminalEvent)?;
        match event {
          crossterm::event::Event::Key(KeyEvent {
            code,
            kind: KeyEventKind::Press,
            ..
          }) => {
            self
              .event_sx
              .send(Event::KeyPressed(code))
              .map_err(|e| AppError::Event(e.to_string()))?;
          }

          crossterm::event::Event::Mouse(MouseEvent { kind, .. }) => match kind {
            MouseEventKind::Down(MouseButton::Left) => {
              left_button_down_at = Some(Instant::now());
            }

            MouseEventKind::Up(MouseButton::Left) => {
              match left_button_down_at {
                Some(when) if when.elapsed() <= Self::CLICK_TIMING => {
                  self
                    .event_sx
                    .send(Event::Click)
                    .map_err(|e| AppError::Event(e.to_string()))?;
                }

                _ => (),
              }

              left_button_down_at = None;
            }

            _ => (),
          },

          _ => (),
        }
      }

      // check for requests
      while let Ok(req) = self.request_rx.try_recv() {
        match req {
          Request::Quit => return Ok(()),
        }
      }

      // TODO: render
    }
  }

  fn render(&self) -> Result<(), AppError> {

    self.terminal.draw(f)
    Ok(())
  }
}

impl Drop for Tui {
  fn drop(&mut self) {
    let _ = self
      .terminal
      .show_cursor()
      .map_err(AppError::TerminalAction);
    let _ =
      execute!(self.terminal.backend_mut(), LeaveAlternateScreen).map_err(AppError::TerminalAction);
    let _ = disable_raw_mode().map_err(AppError::Termination);
  }
}
