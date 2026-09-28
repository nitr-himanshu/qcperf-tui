use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

const HELP: &str = "\
Dashboards
  Up/Down or Left/Right  select
  Tab      next dashboard
  Enter    open
  n        new dashboard
  d        delete a stopped dashboard
  q        quit

Live view
  s        start profiling
  x        stop this dashboard only
  e        edit metrics, chart, color, window, and rates
  Tab or Left/Right   switch dashboard (profiling keeps running)
  PageUp/PageDown     select the previous or next chart
  One chart fills the live view at a time
  c        write the current samples to CSV
  v        toggle appending CSV while this dashboard runs
  p        snapshot to JSON without stopping
  Esc      back to the list
  q        quit

Editor
  Up/Down  move focus through metrics
  Space    toggle the focused metric
  t        switch chart between line and bar
  c        next color
  w        next time window (15s, 30s, 60s, 5min)
  Left/Right          step the sampling rate
  Shift+Left/Right    step the streaming rate
  Enter    save
  Esc      cancel

Metrics default to a scrolling line, and can be switched to bars. The line axis waits for two samples before choosing a data range. Rates come from the capability lists libqcperf reported and apply to every graph of that capability.
";

pub fn render(frame: &mut Frame, area: Rect) {
    let paragraph = Paragraph::new(HELP)
        .block(Block::default().borders(Borders::ALL).title("Help"))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}
