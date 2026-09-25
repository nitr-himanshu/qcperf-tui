use std::time::{Duration, SystemTime};

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Color;
use ratatui::Frame;

use crate::model::{Capability, ChartKind, Dashboard, GraphSpec};
use crate::model::unit::is_percent;
use crate::theme::Theme;
use crate::ui::widgets::{BarSeries, LineSeries, PieChart, PieSlice};

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
) {
    let cells = build_cells(capabilities, theme, dashboard, series);
    if cells.is_empty() {
        return;
    }
    let rects = grid_rects(area, cells.len());
    for (cell, rect) in cells.iter().zip(rects) {
        match &cell.kind {
            CellKind::Pie { title, slices } => {
                frame.render_widget(
                    PieChart {
                        title,
                        slices,
                    },
                    rect,
                );
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
                let value = points.last().map(|(_, value)| *value).unwrap_or(0.0);
                slices.push(PieSlice {
                    label: format!("{name} {value:.1}{unit}"),
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
        .unwrap_or_else(|| format!("backend {} capability {}", graph.backend_id, graph.capability_id))
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
    let cols = columns(area.width).min(count).max(1);
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
