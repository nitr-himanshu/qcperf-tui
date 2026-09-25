use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::model::{Dashboard, RunState};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    dashboards: &[Dashboard],
    selected: usize,
    init_error: Option<&str>,
) {
    let items: Vec<ListItem> = if dashboards.is_empty() {
        vec![ListItem::new(init_error.unwrap_or("No dashboards"))]
    } else {
        dashboards
            .iter()
            .map(|dashboard| {
                let mark = match dashboard.run {
                    RunState::Running => "●",
                    RunState::Stopped => "○",
                    RunState::Idle => "·",
                };
                ListItem::new(Line::from(vec![
                    Span::raw(format!("{mark} {}  ", dashboard.name)),
                    Span::styled(
                        dashboard.run.label(),
                        Style::default().add_modifier(Modifier::DIM),
                    ),
                    Span::raw(format!("  {} graphs", dashboard.graphs.len())),
                ]))
            })
            .collect()
    };
    let mut state = ListState::default();
    if !dashboards.is_empty() {
        state.select(Some(selected.min(dashboards.len() - 1)));
    }
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Dashboards"),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut state);
}
