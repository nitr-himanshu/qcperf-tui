use std::time::{Duration, SystemTime};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Bar, BarChart, BarGroup, Block, Borders, Widget};

use super::format_value;

pub struct BarSeries<'a> {
    pub title: &'a str,
    pub unit: &'a str,
    pub color: Color,
    pub window: Duration,
    pub points: &'a [(SystemTime, f64)],
}

impl Widget for BarSeries<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let buckets = (area.width as usize / 3).clamp(1, 40);
        let values = bucket(self.points, buckets);
        let reading = self
            .points
            .iter()
            .rev()
            .find_map(|(_, value)| value.is_finite().then_some(*value))
            .map(|value| format!("{} {}", format_value(value), self.unit))
            .unwrap_or_else(|| "no data".to_string());
        let bars: Vec<Bar> = values
            .iter()
            .map(|value| {
                Bar::default()
                    .value(value.max(0.0).round() as u64)
                    .style(Style::default().fg(self.color))
            })
            .collect();
        let title = format!(
            "{}  {}  {}s",
            self.title,
            reading,
            self.window.as_secs().max(1)
        );
        let chart = BarChart::default()
            .block(Block::default().borders(Borders::ALL).title(title))
            .data(BarGroup::default().bars(&bars))
            .bar_width(2)
            .bar_gap(1)
            .bar_style(Style::default().fg(self.color));
        chart.render(area, buf);
    }
}

fn bucket(points: &[(SystemTime, f64)], buckets: usize) -> Vec<f64> {
    if points.is_empty() || buckets == 0 {
        return vec![0.0; buckets.max(1)];
    }
    let start = points.len().saturating_sub(buckets);
    let slice = &points[start..];
    if slice.len() >= buckets {
        let chunk = slice.len() / buckets;
        return (0..buckets)
            .map(|index| {
                let from = index * chunk;
                let to = if index + 1 == buckets {
                    slice.len()
                } else {
                    (index + 1) * chunk
                };
                average(&slice[from..to])
            })
            .collect();
    }
    let mut values = vec![0.0; buckets];
    let offset = buckets - slice.len();
    for (index, (_, value)) in slice.iter().enumerate() {
        if value.is_finite() {
            values[offset + index] = *value;
        }
    }
    values
}

fn average(points: &[(SystemTime, f64)]) -> f64 {
    let finite: Vec<f64> = points
        .iter()
        .map(|(_, value)| *value)
        .filter(|value| value.is_finite())
        .collect();
    if finite.is_empty() {
        0.0
    } else {
        let scale = finite.iter().map(|value| value.abs()).fold(0.0, f64::max);
        if scale == 0.0 {
            return 0.0;
        }
        (finite.iter().map(|value| value / scale).sum::<f64>() / finite.len() as f64) * scale
    }
}
