mod editor;
mod help;
mod list;
mod view;
mod widgets;

pub use editor::{Editor, EditorEffect};
pub use help::render as render_help;
pub use list::render as render_list;
pub use view::render as render_view;

use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);
    frame.render_widget(Paragraph::new(header(app)), chunks[0]);
    match app.screen() {
        crate::app::Screen::List => render_list(
            frame,
            chunks[1],
            app.dashboards(),
            app.selected(),
            app.init_error(),
        ),
        crate::app::Screen::View => {
            if let Some(dashboard) = app.selected_dashboard().cloned() {
                let series: Vec<_> = dashboard
                    .graphs
                    .iter()
                    .map(|graph| app.points(dashboard.id, graph.metric_id))
                    .collect();
                render_view(
                    frame,
                    chunks[1],
                    app.capabilities(),
                    app.theme(),
                    &dashboard,
                    &series,
                );
            }
        }
        crate::app::Screen::Edit => {
            let capabilities = app.capabilities().to_vec();
            if let Some(editor) = app.editor() {
                editor::render(frame, chunks[1], editor, &capabilities);
            }
        }
        crate::app::Screen::Help => render_help(frame, chunks[1]),
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            app.status(),
            Style::default().add_modifier(Modifier::DIM),
        ))),
        chunks[2],
    );
}

fn header(app: &App) -> Line<'static> {
    let focus = app
        .selected_dashboard()
        .map(|dashboard| {
            format!(
                "{}  {}  {}",
                dashboard.name,
                dashboard.run.label(),
                rate_summary(dashboard)
            )
        })
        .unwrap_or_else(|| "qcperf-tui".to_string());
    let hint = match app.screen() {
        crate::app::Screen::List => "Enter open  n new  d delete  ? help  q quit",
        crate::app::Screen::View => "s start  x stop  e edit  c csv  v follow  p snapshot  ? help",
        crate::app::Screen::Edit => "Space toggle  t chart  c color  w window  arrows rates  Enter save",
        crate::app::Screen::Help => "Esc back",
    };
    Line::from(format!("{focus}    {hint}"))
}

fn rate_summary(dashboard: &crate::model::Dashboard) -> String {
    dashboard
        .rates
        .iter()
        .map(|rate| {
            format!(
                "b{}:c{} {}/{}ms",
                rate.backend_id, rate.capability_id, rate.sampling_rate_ms, rate.streaming_rate_ms
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}
