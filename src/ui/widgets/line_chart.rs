use std::time::{Duration, SystemTime};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::symbols;
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Widget};

use super::format_value;

pub struct LineSeries<'a> {
    pub title: &'a str,
    pub unit: &'a str,
    pub color: Color,
    pub window: Duration,
    pub points: &'a [(SystemTime, f64)],
    pub percent: bool,
}

impl Widget for LineSeries<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let window_secs = self.window.as_secs().max(1);
        let latest = self
            .points
            .iter()
            .rev()
            .find_map(|(_, value)| value.is_finite().then_some(*value));
        let reading = latest
            .map(|value| format!("{} {}", format_value(value), self.unit))
            .unwrap_or_else(|| "no data".to_string());
        let title = format!("{}  {}  {}s", self.title, reading, window_secs);
        let newest = self
            .points
            .last()
            .map(|(at, _)| *at)
            .unwrap_or_else(SystemTime::now);
        let data: Vec<(f64, f64)> = self
            .points
            .iter()
            .filter_map(|(at, value)| {
                if !value.is_finite() {
                    return None;
                }
                let age = newest.duration_since(*at).unwrap_or_default().as_secs_f64();
                Some((-age, *value))
            })
            .collect();
        let (y_min, y_max) = y_bounds(&data, self.percent);
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
                    .bounds([y_min, y_max])
                    .labels([format_value(y_min), format_value(y_max)]),
            );
        chart.render(area, buf);
    }
}

fn y_bounds(data: &[(f64, f64)], percent: bool) -> (f64, f64) {
    if percent {
        return (0.0, 100.0);
    }
    let Some((min, max)) = data
        .iter()
        .map(|(_, value)| *value)
        .filter(|value| value.is_finite())
        .fold(None::<(f64, f64)>, |bounds, value| {
            Some(match bounds {
                Some((min, max)) => (min.min(value), max.max(value)),
                None => (value, value),
            })
        })
    else {
        return (0.0, 1.0);
    };

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
        (low, high)
    } else {
        (0.0, 1.0)
    }
}
