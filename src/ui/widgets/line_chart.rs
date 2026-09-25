use std::time::{Duration, SystemTime};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::symbols;
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Widget};

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
        let title = format!("{}  {}s", self.title, window_secs);
        let newest = self
            .points
            .last()
            .map(|(at, _)| *at)
            .unwrap_or_else(SystemTime::now);
        let data: Vec<(f64, f64)> = self
            .points
            .iter()
            .map(|(at, value)| {
                let age = newest.duration_since(*at).unwrap_or_default().as_secs_f64();
                (-age, *value)
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
                    .labels([format!("{y_min:.1}"), format!("{y_max:.1}")]),
            );
        chart.render(area, buf);
    }
}

fn y_bounds(data: &[(f64, f64)], percent: bool) -> (f64, f64) {
    if percent {
        return (0.0, 100.0);
    }
    let mut min = f64::MAX;
    let mut max = f64::MIN;
    for (_, value) in data {
        min = min.min(*value);
        max = max.max(*value);
    }
    if !min.is_finite() || !max.is_finite() {
        return (0.0, 1.0);
    }
    if (max - min).abs() < f64::EPSILON {
        let pad = min.abs().max(1.0) * 0.1;
        return (min - pad, max + pad);
    }
    let pad = (max - min) * 0.1;
    (min - pad, max + pad)
}
