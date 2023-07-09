use std::time::Instant;

use tui::text::Span;

/// Messages that won’t go out of the UI unless a given timeout is reached.
#[derive(Debug)]
pub struct StickyMsg {
  span: Span<'static>,
  until: Instant,
}

impl StickyMsg {
  pub fn new(span: impl Into<Span<'static>>, until: Instant) -> Self {
    Self {
      span: span.into(),
      until,
    }
  }

  pub fn span(&self) -> &Span {
    &self.span
  }

  pub fn until(&self) -> Instant {
    self.until
  }
}
