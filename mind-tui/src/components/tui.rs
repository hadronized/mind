use std::{
  io::Stdout,
  sync::mpsc::{Receiver, Sender},
  time::{Duration, Instant},
};

use crossterm::{
  execute,
  terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use mind_tree::config::Config;
use tui::{
  backend::CrosstermBackend,
  layout::Rect,
  style::{Color, Style},
  text::Span,
  Terminal,
};

use crate::{
  error::AppError,
  event::{Event, HandledEvent, RawEventHandler},
  ops::InsertMode,
  req::Request,
};

use super::{
  cmd_line::CmdLine,
  editor::Editor,
  menu::{MenuItem, TuiMenu},
  sticky_msg::StickyMsg,
  tree::TuiTree,
  user_input::UserInputPrompt,
};

pub struct Tui {
  terminal: Terminal<CrosstermBackend<Stdout>>,
  request_rx: Receiver<Request>,

  // components
  cmd_line: CmdLine,
  tree: TuiTree,
  sticky_msg: Option<StickyMsg>,
  menu: TuiMenu,
  prompt: UserInputPrompt,
  editor: Editor,
}

impl Tui {
  /// Allow errors to occur and display them in case of occurrence.
  pub fn display_errors<T>(&mut self, a: Result<T, AppError>, f: impl FnOnce(T)) {
    match a {
      Ok(a) => f(a),
      Err(err) => self.display_sticky(
        Span::styled(err.to_string(), Style::default().fg(Color::Red)),
        Duration::from_secs(5),
      ),
    }
  }

  /// Display a message with a given style and stick it around.
  ///
  /// The message will stick around until its timeout time is reached. If a message was still there, it is replaced
  /// with the new message.
  fn display_sticky(&mut self, span: impl Into<Span<'static>>, timeout: Duration) {
    self.sticky_msg = Instant::now()
      .checked_add(timeout)
      .map(move |until| StickyMsg::new(span, until));
  }

  pub fn new(
    config: &Config,
    mut tree: TuiTree,
    event_sx: Sender<Event>,
    request_rx: Receiver<Request>,
  ) -> Result<Self, AppError> {
    enable_raw_mode().map_err(AppError::Init)?;

    let mut stdout = std::io::stdout();
    execute!(&mut stdout, EnterAlternateScreen).map_err(AppError::TerminalAction)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout)).map_err(AppError::Init)?;

    terminal.hide_cursor().map_err(AppError::TerminalAction)?;

    tree.set_area(terminal.get_frame().size());

    let cmd_line = CmdLine::new(event_sx);
    let sticky_msg = None;
    let menu = TuiMenu::default();
    let prompt = UserInputPrompt::default();
    let editor = Editor::new(config)?;

    Ok(Tui {
      terminal,
      request_rx,
      cmd_line,
      tree,
      sticky_msg,
      menu,
      prompt,
      editor,
    })
  }

  /// Open the menu with the provided list of items and send the result on the given channel.
  fn open_menu(
    &mut self,
    title: impl Into<String>,
    items: impl Into<Vec<MenuItem>>,
    sender: Sender<Option<MenuItem>>,
  ) {
    self.menu.show(title, items, sender);
  }

  /// Open the prompt with the provided title and send the result on the given channel.
  fn open_prompt(&mut self, title: impl Into<String>, sender: Sender<Option<String>>) {
    self.prompt.show_with_title(title, sender);
  }

  fn refresh(&mut self) {
    // check whether we should remove sticky messages
    if let Some(ref mut sticky_msg) = self.sticky_msg {
      if sticky_msg.until() < Instant::now() {
        self.sticky_msg = None;
      }
    }
  }

  fn render(&mut self) -> Result<(), AppError> {
    self
      .terminal
      .draw(|f| {
        let size = f.size();

        // when some widgets are active, this value is negative so that we do not overlap with the widgets active at
        // the bottom of the screen
        let mut tree_height_bias = 0;

        // render the command line, if any
        if let Some(prompt) = self.cmd_line.prompt() {
          let y = size.height - 1;
          tree_height_bias = 1;

          // render the line
          f.render_widget(
            prompt,
            Rect {
              y,
              height: 1,
              ..size
            },
          );

          // position the cursor
          f.set_cursor(size.x + 1 + prompt.cursor() as u16, y);
        }

        // render the menu if any
        if self.menu.is_visible() {
          let height = self.menu.height().min(size.height / 3);
          let area = Rect {
            x: size.x,
            y: size.height - height,
            width: size.width,
            height,
          };

          f.render_widget(&self.menu, area);

          tree_height_bias = height;
        }

        // render the prompt, if any
        if let Some(prompt) = self.prompt.prompt() {
          let y = size.height - 1;
          tree_height_bias = 1;

          f.render_widget(
            prompt,
            Rect {
              y,
              height: 1,
              ..size
            },
          );
        }

        // render the tree
        let tree_area = self.tree.area();
        f.render_widget(
          &self.tree,
          Rect {
            height: tree_area.height - tree_height_bias,
            ..*tree_area
          },
        );

        // render any sticky message, if any
        if let Some(ref sticky_msg) = self.sticky_msg {
          let p = tui::widgets::Paragraph::new(sticky_msg.span().content.as_ref())
            .style(sticky_msg.span().style)
            .wrap(tui::widgets::Wrap { trim: false })
            .alignment(tui::layout::Alignment::Right);
          let width = size.width / 4;
          let height = size.height / 2;
          f.render_widget(
            p,
            Rect {
              x: size.x + 3 * width,
              y: size.y,
              width,
              height,
            },
          );
        }
      })
      .map(|_| ())
      .map_err(AppError::Render)
  }

  pub fn run(mut self) -> Result<(), AppError> {
    let mut needs_redraw = true;

    loop {
      // event available
      let available_event = crossterm::event::poll(Duration::from_millis(50))
        .map_err(|e| AppError::Event(e.to_string()))?;

      if available_event {
        let event = crossterm::event::read().map_err(AppError::TerminalEvent)?;
        let handled_event = self.react_raw(event).map(|(handled, _)| handled);

        self.display_errors(handled_event, |event| {
          if let HandledEvent::Handled { requires_redraw } = event {
            needs_redraw |= requires_redraw;
          }
        });
      }

      // check for requests
      while let Ok(req) = self.request_rx.try_recv() {
        match req {
          Request::NewTree(tree) => {
            self.tree = tree;
            self.tree.set_area(self.terminal.get_frame().size());
          }

          Request::StickyMsg { span, timeout } => {
            self.display_sticky(span, timeout);
          }

          Request::Quit => return Ok(()),

          Request::InsertedNode { mode, .. } => {
            if let InsertMode::Before = mode {
              self.tree.shift_selected_node_id(1);
            }
          }

          Request::DeletedNode { .. } => {
            log::debug!("adapting TUI to deleted node…");

            // the node doesn’t exist anymore, so we try to move to the next node, or to the previous and update the
            // selected node ID
            if self.tree.select_next_node() {
              self.tree.shift_selected_node_id(-1);
            } else {
              self.tree.select_prev_node();
            }
          }

          // HACK: this is a bit hacky… we need that just to ask the TUI to refresh, which is a bit weird
          Request::RenamedNode { .. } => {
            log::debug!("adapting TUI to renamed node…");
          }

          Request::PromptNodeData { sender } => {
            self.open_menu(
              "create data",
              [MenuItem::new("file", 'f'), MenuItem::new("url", 'u')],
              sender,
            );
          }

          Request::UserInput { title, sender } => self.open_prompt(title, sender),

          Request::OpenEditor { path } => {
            self.editor.edit(&path)?;
            self.terminal.clear().map_err(AppError::TerminalAction)?;
          }
        }

        needs_redraw = true;
      }

      self.refresh();

      // render
      if needs_redraw {
        self.render()?;
        needs_redraw = false;
      }
    }
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

impl RawEventHandler for Tui {
  type Feedback = ();

  fn react_raw(
    &mut self,
    event: crossterm::event::Event,
  ) -> Result<(HandledEvent, Self::Feedback), AppError> {
    let handled = self
      .menu
      .react_raw(event)
      .map(|(handled, _)| handled)?
      .and_then(&mut self.prompt)?
      .and_then(&mut self.cmd_line)?
      .and_then(&mut self.tree)?;
    Ok((handled, ()))
  }
}
