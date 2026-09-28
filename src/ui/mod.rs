mod editor;
mod help;
mod list;
mod view;
mod widgets;

pub use editor::{Editor, EditorEffect};
pub use help::render as render_help;
pub use list::render as render_list;
pub use view::{
    chart_count as view_chart_count, page_capacity as view_page_capacity, render as render_view,
};

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
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);
    frame.render_widget(Paragraph::new(header(app)), chunks[0]);
    frame.render_widget(Paragraph::new(key_hints(app)), chunks[1]);
    match app.screen() {
        crate::app::Screen::List => render_list(
            frame,
            chunks[2],
            app.dashboards(),
            app.selected(),
            app.init_error(),
        ),
        crate::app::Screen::View => {
            if let Some(dashboard) = app.selected_dashboard().cloned() {
                let series: Vec<_> = dashboard
                    .graphs
                    .iter()
                    .map(|graph| {
                        app.points(
                            dashboard.id,
                            graph.backend_id,
                            graph.capability_id,
                            graph.metric_id,
                        )
                    })
                    .collect();
                render_view(
                    frame,
                    chunks[2],
                    app.capabilities(),
                    app.theme(),
                    &dashboard,
                    &series,
                    app.view_page(),
                );
            }
        }
        crate::app::Screen::Edit => {
            let capabilities = app.capabilities().to_vec();
            if let Some(editor) = app.editor() {
                editor::render(frame, chunks[2], editor, &capabilities);
            }
        }
        crate::app::Screen::Help => render_help(frame, chunks[2]),
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            app.status(),
            Style::default().add_modifier(Modifier::DIM),
        ))),
        chunks[3],
    );
}

fn header(app: &App) -> Line<'static> {
    let focus = app
        .selected_dashboard()
        .map(|dashboard| {
            format!("{}  {}", dashboard.name, dashboard.run.label())
        })
        .unwrap_or_else(|| "qcperf-tui".to_string());
    Line::from(focus)
}

fn key_hints(app: &App) -> Line<'static> {
    let hint = match app.screen() {
        crate::app::Screen::List => "↑/↓ select  Enter open  n new  d delete  ? help  q quit",
        crate::app::Screen::View => {
            "s start  x stop  e edit  c CSV  p snapshot  PgUp/Dn charts  Tab/←/→ dashboards"
        }
        crate::app::Screen::Edit => {
            "↑/↓ focus  Space toggle  t chart  c color  w window  ←/→ sample  Enter save"
        }
        crate::app::Screen::Help => "Esc back",
    };
    Line::from(hint)
}
