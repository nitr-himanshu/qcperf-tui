use std::time::SystemTime;

use ratatui::layout::Rect;
use ratatui::Frame;

use crate::model::{Capability, ChartKind, Dashboard, GraphSpec};
use crate::theme::Theme;
use crate::ui::widgets::{BarSeries, LineSeries};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    capabilities: &[Capability],
    theme: &Theme,
    dashboard: &Dashboard,
    series: &[Vec<(SystemTime, f64)>],
    chart: usize,
) {
    let Some(graph) = dashboard.graphs.get(chart) else {
        frame.render_widget(
            ratatui::widgets::Paragraph::new(
                "No metrics selected. Press e to choose metrics for this dashboard.",
            )
            .block(
                ratatui::widgets::Block::default()
                    .borders(ratatui::widgets::Borders::ALL)
                    .title("Dashboard"),
            )
            .wrap(ratatui::widgets::Wrap { trim: true }),
            area,
        );
        return;
    };

    let (name, unit) = metric_text(capabilities, graph);
    let title = format!("{}  [{}/{}]", name, chart + 1, dashboard.graphs.len());
    let points = series.get(chart).map(Vec::as_slice).unwrap_or(&[]);
    let color = theme.color(graph.color.index);
    match graph.chart {
        ChartKind::Bar => frame.render_widget(
            BarSeries {
                title: &title,
                unit: &unit,
                color,
                window: graph.window,
                points,
            },
            area,
        ),
        ChartKind::Line => frame.render_widget(
            LineSeries {
                title: &title,
                unit: &unit,
                color,
                window: graph.window,
                points,
            },
            area,
        ),
    }
}

/// A dashboard shows one chart at a time; the chart fills the live view.
pub fn page_capacity(_width: u16, _height: u16) -> usize {
    1
}

pub fn chart_count(dashboard: &Dashboard) -> usize {
    dashboard.graphs.len()
}

fn metric_text(capabilities: &[Capability], graph: &GraphSpec) -> (String, String) {
    let metric = capabilities.iter().find_map(|cap| {
        (cap.backend_id == graph.backend_id && cap.capability_id == graph.capability_id)
            .then(|| cap.metric(graph.metric_id))
            .flatten()
    });
    match metric {
        Some(metric) => (metric.name.clone(), metric.unit.clone()),
        None => (format!("metric {}", graph.metric_id), String::new()),
    }
}
