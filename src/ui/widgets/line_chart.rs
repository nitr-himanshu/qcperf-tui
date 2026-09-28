use std::time::{Duration, SystemTime};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::symbols;
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Widget};

use super::format_value;

const MIN_AXIS_SAMPLES: usize = 2;
const Y_AXIS_LABEL_WIDTH: u16 = 12;
const BRAILLE_PIXELS_PER_CELL: usize = 2;
const PIXELS_PER_SAMPLE: usize = 1;

pub struct LineSeries<'a> {
    pub title: &'a str,
    pub unit: &'a str,
    pub color: Color,
    pub window: Duration,
    pub points: &'a [(SystemTime, f64)],
}

impl Widget for LineSeries<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let window_secs = self.window.as_secs().max(1);
        let finite: Vec<(SystemTime, f64)> = self
            .points
            .iter()
            .copied()
            .filter(|(_, value)| value.is_finite())
            .collect();
        let latest = finite.last().map(|(_, value)| *value);
        let ready = finite.len() >= MIN_AXIS_SAMPLES;
        let reading = if ready {
            latest
                .map(|value| format!("{} {}", format_value(value), self.unit))
                .unwrap_or_else(|| "no data".to_string())
        } else {
            format!("collecting samples {}/{}", finite.len(), MIN_AXIS_SAMPLES)
        };
        let title = format!("{}  {}  {}s", self.title, reading, window_secs);
        let newest = self
            .points
            .last()
            .map(|(at, _)| *at)
            .unwrap_or_else(SystemTime::now);
        let raw_data: Vec<(f64, f64)> = finite
            .iter()
            .map(|(at, value)| {
                let age = newest.duration_since(*at).unwrap_or_default().as_secs_f64();
                (-age, *value)
            })
            .collect();
        let capacity = sample_capacity(area);
        let data = if ready {
            downsample(&raw_data, capacity)
        } else {
            Vec::new()
        };
        let bounds = if ready {
            y_bounds(&raw_data).unwrap_or((0.0, 1.0))
        } else {
            (0.0, 1.0)
        };
        let y_labels = if ready {
            vec![format_value(bounds.0), format_value(bounds.1)]
        } else {
            vec!["—".to_string(), "—".to_string()]
        };
        let datasets = vec![Dataset::default()
            .name(self.unit)
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(self.color))
            .data(&data)];
        let chart = Chart::new(datasets)
            .block(Block::default().borders(Borders::ALL).title(title))
            .x_axis(
                Axis::default()
                    .title(format!("{window_secs}s"))
                    .bounds([-(window_secs as f64), 0.0])
                    .labels(["", "now"]),
            )
            .y_axis(
                Axis::default()
                    .title(self.unit)
                    .bounds([bounds.0, bounds.1])
                    .labels(y_labels),
            );
        chart.render(area, buf);
    }
}

/// The line plot uses two horizontal Braille pixels per terminal cell. Each
/// plotted sample consumes one pixel, so this caps plotted points at the
/// available pixel count (total pixels / pixels per sample).
fn sample_capacity(area: Rect) -> usize {
    let plot_cells = area.width.saturating_sub(Y_AXIS_LABEL_WIDTH) as usize;
    let total_pixels = plot_cells.saturating_mul(BRAILLE_PIXELS_PER_CELL);
    (total_pixels / PIXELS_PER_SAMPLE).max(1)
}

fn downsample(data: &[(f64, f64)], capacity: usize) -> Vec<(f64, f64)> {
    if data.len() <= capacity {
        return data.to_vec();
    }

    (0..capacity)
        .filter_map(|bucket| {
            let start = bucket * data.len() / capacity;
            let end = ((bucket + 1) * data.len() / capacity).max(start + 1);
            let points = &data[start..end.min(data.len())];
            if points.is_empty() {
                return None;
            }
            let x = points.iter().map(|(x, _)| *x).sum::<f64>() / points.len() as f64;
            let scale = points
                .iter()
                .map(|(_, value)| value.abs())
                .fold(0.0, f64::max);
            let y = if scale == 0.0 {
                0.0
            } else {
                (points.iter().map(|(_, value)| value / scale).sum::<f64>()
                    / points.len() as f64)
                    * scale
            };
            Some((x, y))
        })
        .collect()
}

fn y_bounds(data: &[(f64, f64)]) -> Option<(f64, f64)> {
    let (min, max) = data
        .iter()
        .map(|(_, value)| *value)
        .filter(|value| value.is_finite())
        .fold(None::<(f64, f64)>, |bounds, value| {
            Some(match bounds {
                Some((min, max)) => (min.min(value), max.max(value)),
                None => (value, value),
            })
        })?;

    let span = max - min;
    let magnitude = min.abs().max(max.abs());
    let pad = if span.is_finite() && span > f64::EPSILON {
        span * 0.1
    } else {
        (magnitude * 0.1).max(1.0)
    };
    let low = (min - pad).max(-f64::MAX);
    let high = (max + pad).min(f64::MAX);
    if low.is_finite() && high.is_finite() && low < high {
        Some((low, high))
    } else {
        Some((0.0, 1.0))
    }
}
