use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::model::{
    default_rate, next_window, Capability, ChartKind, Dashboard, GraphSpec, SeriesColor,
};

pub struct Editor {
    pub dashboard: Dashboard,
    pub cursor: usize,
    pub active: bool,
}

impl Editor {
    pub fn open(dashboard: Dashboard) -> Self {
        Self {
            dashboard,
            cursor: 0,
            active: true,
        }
    }

    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        capabilities: &[Capability],
        rate_locked: bool,
    ) -> EditorEffect {
        let rows = metric_rows(capabilities);
        if rows.is_empty() {
            return match key.code {
                KeyCode::Esc => EditorEffect::Cancel,
                _ => EditorEffect::None,
            };
        }
        self.cursor = self.cursor.min(rows.len() - 1);
        let row = &rows[self.cursor];
        match key.code {
            KeyCode::Esc => EditorEffect::Cancel,
            KeyCode::Enter => EditorEffect::Saved,
            KeyCode::Up => {
                self.cursor = self.cursor.saturating_sub(1);
                EditorEffect::None
            }
            KeyCode::Down => {
                if self.cursor + 1 < rows.len() {
                    self.cursor += 1;
                }
                EditorEffect::None
            }
            KeyCode::Char(' ') => {
                self.toggle(row, capabilities);
                EditorEffect::None
            }
            KeyCode::Char('t') => {
                self.cycle_chart(row);
                EditorEffect::None
            }
            KeyCode::Char('c') => {
                self.cycle_color(row);
                EditorEffect::None
            }
            KeyCode::Char('w') => {
                self.cycle_window(row);
                EditorEffect::None
            }
            KeyCode::Left | KeyCode::Right if rate_locked => EditorEffect::Status(
                "that capability is streaming on another dashboard; stop it there before changing the rate".to_string(),
            ),
            KeyCode::Left => {
                let step = if key.modifiers.contains(KeyModifiers::SHIFT) {
                    RateField::Stream
                } else {
                    RateField::Sample
                };
                self.step_rate(row, capabilities, step, -1);
                EditorEffect::None
            }
            KeyCode::Right => {
                let step = if key.modifiers.contains(KeyModifiers::SHIFT) {
                    RateField::Stream
                } else {
                    RateField::Sample
                };
                self.step_rate(row, capabilities, step, 1);
                EditorEffect::None
            }
            _ => EditorEffect::None,
        }
    }

    fn toggle(&mut self, row: &MetricRow, capabilities: &[Capability]) {
        if let Some(index) = self.graph_index(row) {
            self.dashboard.graphs.remove(index);
            let still = self.dashboard.graphs.iter().any(|graph| {
                graph.backend_id == row.backend_id && graph.capability_id == row.capability_id
            });
            if !still {
                self.dashboard
                    .rates
                    .retain(|rate| {
                        !(rate.backend_id == row.backend_id
                            && rate.capability_id == row.capability_id)
                    });
            }
            return;
        }
        let Some(capability) = capabilities.iter().find(|cap| {
            cap.backend_id == row.backend_id && cap.capability_id == row.capability_id
        }) else {
            return;
        };
        let Some(metric) = capability.metric(row.metric_id) else {
            return;
        };
        if self.dashboard.rate(row.backend_id, row.capability_id).is_none() {
            if let Some(rate) = default_rate(capability) {
                self.dashboard.rates.push(rate);
            }
        }
        let color = self.dashboard.graphs.len() as u8;
        self.dashboard.graphs.push(GraphSpec {
            backend_id: row.backend_id,
            capability_id: row.capability_id,
            metric_id: row.metric_id,
            chart: ChartKind::default_for(&metric.unit),
            color: SeriesColor::new(color),
            window: Duration::from_secs(60),
        });
    }

    fn cycle_chart(&mut self, row: &MetricRow) {
        if let Some(graph) = self.graph_mut(row) {
            graph.chart = graph.chart.next();
        }
    }

    fn cycle_color(&mut self, row: &MetricRow) {
        if let Some(graph) = self.graph_mut(row) {
            graph.color = graph.color.next();
        }
    }

    fn cycle_window(&mut self, row: &MetricRow) {
        if let Some(graph) = self.graph_mut(row) {
            graph.window = next_window(graph.window);
        }
    }

    fn step_rate(&mut self, row: &MetricRow, capabilities: &[Capability], field: RateField, dir: i32) {
        let Some(capability) = capabilities.iter().find(|cap| {
            cap.backend_id == row.backend_id && cap.capability_id == row.capability_id
        }) else {
            return;
        };
        let Some(rate) = self
            .dashboard
            .rates
            .iter_mut()
            .find(|rate| rate.backend_id == row.backend_id && rate.capability_id == row.capability_id)
        else {
            return;
        };
        match field {
            RateField::Sample => step_list(&capability.sampling_rates_ms, &mut rate.sampling_rate_ms, dir),
            RateField::Stream => step_list(&capability.streaming_rates_ms, &mut rate.streaming_rate_ms, dir),
        }
        if rate.streaming_rate_ms < rate.sampling_rate_ms {
            if let Some(stream) = capability
                .streaming_rates_ms
                .iter()
                .copied()
                .find(|value| *value >= rate.sampling_rate_ms)
            {
                rate.streaming_rate_ms = stream;
            }
        }
    }

    fn graph_index(&self, row: &MetricRow) -> Option<usize> {
        self.dashboard.graphs.iter().position(|graph| {
            graph.backend_id == row.backend_id
                && graph.capability_id == row.capability_id
                && graph.metric_id == row.metric_id
        })
    }

    fn graph_mut(&mut self, row: &MetricRow) -> Option<&mut GraphSpec> {
        self.dashboard.graphs.iter_mut().find(|graph| {
            graph.backend_id == row.backend_id
                && graph.capability_id == row.capability_id
                && graph.metric_id == row.metric_id
        })
    }
}

enum RateField {
    Sample,
    Stream,
}

pub enum EditorEffect {
    None,
    Cancel,
    Saved,
    Status(String),
}

struct MetricRow {
    backend_id: u8,
    capability_id: u8,
    metric_id: u16,
    label: String,
    unit: String,
}

fn metric_rows(capabilities: &[Capability]) -> Vec<MetricRow> {
    let mut rows = Vec::new();
    for capability in capabilities {
        for metric in &capability.metrics {
            rows.push(MetricRow {
                backend_id: capability.backend_id,
                capability_id: capability.capability_id,
                metric_id: metric.metric_id,
                label: format!("{} / {}", capability.label(), metric.name),
                unit: metric.unit.clone(),
            });
        }
    }
    rows
}

fn step_list(values: &[u16], current: &mut u16, dir: i32) {
    if values.is_empty() {
        return;
    }
    let index = values.iter().position(|value| *value == *current).unwrap_or(0) as i32;
    let next = index + dir;
    if next >= 0 && (next as usize) < values.len() {
        *current = values[next as usize];
    }
}

pub fn render(frame: &mut Frame, area: Rect, editor: &Editor, capabilities: &[Capability]) {
    let rows = metric_rows(capabilities);
    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| {
            let selected = editor.dashboard.contains_metric(row.backend_id, row.capability_id, row.metric_id);
            let mark = if selected { "[x]" } else { "[ ]" };
            let detail = editor
                .dashboard
                .graphs
                .iter()
                .find(|graph| {
                    graph.backend_id == row.backend_id
                        && graph.capability_id == row.capability_id
                        && graph.metric_id == row.metric_id
                })
                .map(|graph| {
                    format!(
                        "  {}  {}s  color {}",
                        graph.chart.label(),
                        graph.window.as_secs(),
                        graph.color.index
                    )
                })
                .unwrap_or_default();
            let rate = editor
                .dashboard
                .rate(row.backend_id, row.capability_id)
                .map(|rate| format!("  sample {} ms  stream {} ms", rate.sampling_rate_ms, rate.streaming_rate_ms))
                .unwrap_or_default();
            ListItem::new(Line::from(Span::raw(format!(
                "{mark} {} ({}){detail}{rate}",
                row.label, row.unit
            ))))
        })
        .collect();
    let mut state = ListState::default();
    if !rows.is_empty() {
        state.select(Some(editor.cursor.min(rows.len() - 1)));
    }
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Edit {}", editor.dashboard.name)),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut state);
}
