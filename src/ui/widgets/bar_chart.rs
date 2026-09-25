use std::time::{Duration, SystemTime};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Bar, BarChart, BarGroup, Block, Borders, Widget};

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
            self.unit,
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
        values[offset + index] = *value;
    }
    values
}

fn average(points: &[(SystemTime, f64)]) -> f64 {
    if points.is_empty() {
        0.0
    } else {
        points.iter().map(|(_, value)| *value).sum::<f64>() / points.len() as f64
    }
}
