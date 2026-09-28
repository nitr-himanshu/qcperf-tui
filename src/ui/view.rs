use std::time::{Duration, SystemTime};

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Color;
use ratatui::Frame;

use crate::model::unit::is_percent;
use crate::model::{Capability, ChartKind, Dashboard, GraphSpec};
use crate::theme::Theme;
use crate::ui::widgets::{format_value, BarSeries, LineSeries, PieChart, PieSlice};

const MAX_CHARTS_PER_PAGE: usize = 4;
const MIN_CHART_HEIGHT: usize = 8;

struct Cell {
    kind: CellKind,
}

enum CellKind {
    Pie {
        title: String,
        slices: Vec<PieSlice>,
    },
    Line {
        title: String,
        unit: String,
        color: Color,
        window: Duration,
        points: Vec<(SystemTime, f64)>,
        percent: bool,
    },
    Bar {
        title: String,
        unit: String,
        color: Color,
        window: Duration,
        points: Vec<(SystemTime, f64)>,
    },
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    capabilities: &[Capability],
    theme: &Theme,
    dashboard: &Dashboard,
    series: &[Vec<(SystemTime, f64)>],
    page: usize,
) {
    let cells = build_cells(capabilities, theme, dashboard, series);
    if cells.is_empty() {
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
    }
    let per_page = page_capacity(area.width, area.height);
    let first = page.min(cells.len().div_ceil(per_page).saturating_sub(1)) * per_page;
    let visible = &cells[first..(first + per_page).min(cells.len())];
    let rects = grid_rects(area, visible.len());
    for (cell, rect) in visible.iter().zip(rects) {
        match &cell.kind {
            CellKind::Pie { title, slices } => {
                frame.render_widget(PieChart { title, slices }, rect);
            }
            CellKind::Line {
                title,
                unit,
                color,
                window,
                points,
                percent,
            } => {
                frame.render_widget(
                    LineSeries {
                        title,
                        unit,
                        color: *color,
                        window: *window,
                        points,
                        percent: *percent,
                    },
                    rect,
                );
            }
            CellKind::Bar {
                title,
                unit,
                color,
                window,
                points,
            } => {
                frame.render_widget(
                    BarSeries {
                        title,
                        unit,
                        color: *color,
                        window: *window,
                        points,
                    },
                    rect,
                );
            }
        }
    }
}

/// Keep charts near eight rows tall when the terminal has enough space.
pub fn page_capacity(width: u16, height: u16) -> usize {
    (columns(width) * (height as usize / MIN_CHART_HEIGHT).max(1)).min(MAX_CHARTS_PER_PAGE)
}

pub fn chart_count(dashboard: &Dashboard) -> usize {
    let mut count = 0;
    let mut index = 0;
    while index < dashboard.graphs.len() {
        let graph = &dashboard.graphs[index];
        count += 1;
        index += 1;
        if graph.chart == ChartKind::Pie {
            while index < dashboard.graphs.len() {
                let next = &dashboard.graphs[index];
                if next.chart != ChartKind::Pie
                    || next.backend_id != graph.backend_id
                    || next.capability_id != graph.capability_id
                {
                    break;
                }
                index += 1;
            }
        }
    }
    count
}

fn build_cells(
    capabilities: &[Capability],
    theme: &Theme,
    dashboard: &Dashboard,
    series: &[Vec<(SystemTime, f64)>],
) -> Vec<Cell> {
    let mut cells = Vec::new();
    let mut index = 0;
    while index < dashboard.graphs.len() {
        let graph = &dashboard.graphs[index];
        if graph.chart == ChartKind::Pie {
            let mut slices = Vec::new();
            let title = capability_label(capabilities, graph);
            while index < dashboard.graphs.len() {
                let next = &dashboard.graphs[index];
                if next.chart != ChartKind::Pie
                    || next.backend_id != graph.backend_id
                    || next.capability_id != graph.capability_id
                {
                    break;
                }
                let (name, unit) = metric_text(capabilities, next);
                let points = series.get(index).map(Vec::as_slice).unwrap_or(&[]);
                let latest = points
                    .iter()
                    .rev()
                    .find_map(|(_, value)| value.is_finite().then_some(*value));
                let value = latest.unwrap_or(0.0);
                let label = latest
                    .map(|value| format!("{name} {}{unit}", format_value(value)))
                    .unwrap_or_else(|| format!("{name} no data"));
                slices.push(PieSlice {
                    label,
                    value,
                    color: theme.color(next.color.index),
                });
                index += 1;
            }
            cells.push(Cell {
                kind: CellKind::Pie { title, slices },
            });
            continue;
        }
        let (name, unit) = metric_text(capabilities, graph);
        let points = series.get(index).cloned().unwrap_or_default();
        let color = theme.color(graph.color.index);
        let cell = if graph.chart == ChartKind::Bar {
            CellKind::Bar {
                title: name,
                unit,
                color,
                window: graph.window,
                points,
            }
        } else {
            CellKind::Line {
                title: name,
                unit: unit.clone(),
                color,
                window: graph.window,
                points,
                percent: is_percent(&unit),
            }
        };
        cells.push(Cell { kind: cell });
        index += 1;
    }
    cells
}

fn capability_label(capabilities: &[Capability], graph: &GraphSpec) -> String {
    capabilities
        .iter()
        .find(|cap| cap.backend_id == graph.backend_id && cap.capability_id == graph.capability_id)
        .map(|cap| cap.label())
        .unwrap_or_else(|| {
            format!(
                "backend {} capability {}",
                graph.backend_id, graph.capability_id
            )
        })
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

pub fn columns(width: u16) -> usize {
    if width < 60 {
        1
    } else if width < 120 {
        2
    } else {
        3
    }
}

fn grid_rects(area: Rect, count: usize) -> Vec<Rect> {
    let preferred_columns = if count >= MAX_CHARTS_PER_PAGE {
        columns(area.width).min(2)
    } else {
        columns(area.width)
    };
    let cols = preferred_columns.min(count).max(1);
    let rows = count.div_ceil(cols);
    let col_constraints = equal_constraints(cols);
    let row_constraints = equal_constraints(rows);
    let row_rects = Layout::vertical(row_constraints).split(area);
    let mut rects = Vec::with_capacity(count);
    for row in row_rects.iter().take(rows) {
        let col_rects = Layout::horizontal(col_constraints.clone()).split(*row);
        for col in col_rects.iter().take(cols) {
            rects.push(*col);
            if rects.len() == count {
                return rects;
            }
        }
    }
    rects
}

fn equal_constraints(n: usize) -> Vec<Constraint> {
    let n = n.max(1) as u16;
    let base = 100 / n;
    let mut constraints = vec![Constraint::Percentage(base); n as usize];
    let used = base * (n - 1);
    if let Some(last) = constraints.last_mut() {
        *last = Constraint::Percentage(100 - used);
    }
    constraints
}
