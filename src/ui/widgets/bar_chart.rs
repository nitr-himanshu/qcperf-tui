use std::time::{Duration, SystemTime};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Bar, BarChart, BarGroup, Block, Borders, Widget};

use super::format_value;

const MIN_AXIS_SAMPLES: usize = 2;
const BAR_SAMPLE_WIDTH: usize = 3;

pub struct BarSeries<'a> {
    pub title: &'a str,
    pub unit: &'a str,
    pub color: Color,
    pub window: Duration,
    pub points: &'a [(SystemTime, f64)],
}

impl Widget for BarSeries<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let buckets = sample_capacity(area);
        let finite: Vec<_> = self
            .points
            .iter()
            .filter(|(_, value)| value.is_finite())
            .copied()
            .collect();
        let ready = finite.len() >= MIN_AXIS_SAMPLES;
        let latest = finite.last().map(|(_, value)| *value);
        let reading = if ready {
            latest
                .map(|value| format!("{} {}", format_value(value), self.unit))
                .unwrap_or_else(|| "no data".to_string())
        } else {
            format!("collecting samples {}/{}", finite.len(), MIN_AXIS_SAMPLES)
        };
        let values = if ready {
            finite
                .last()
                .map(|(newest, _)| bucket(&finite, buckets, *newest, self.window))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let max_value = values
            .iter()
            .map(|value| value.max(0.0))
            .fold(0.0, f64::max);
        let bars: Vec<Bar> = values
            .iter()
            .map(|value| {
                Bar::default()
                    .value(if max_value > 0.0 {
                        (value.max(0.0) / max_value * 1_000_000.0).round() as u64
                    } else {
                        0
                    })
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

/// Each bar consumes three terminal columns (bar width plus gap). The visible
/// sample count is the available plot width divided by that per-sample width.
fn sample_capacity(area: Rect) -> usize {
    ((area.width.saturating_sub(2) as usize) / BAR_SAMPLE_WIDTH).max(1)
}

fn bucket(
    points: &[(SystemTime, f64)],
    buckets: usize,
    newest: SystemTime,
    window: Duration,
) -> Vec<f64> {
    if points.is_empty() || buckets == 0 {
        return Vec::new();
    }
    let window_secs = window.as_secs_f64().max(1.0);
    let mut samples = vec![Vec::new(); buckets];
    for (at, value) in points.iter().filter(|(_, value)| value.is_finite()) {
        let age = newest.duration_since(*at).unwrap_or_default().as_secs_f64();
        if age > window_secs {
            continue;
        }
        let position = (1.0 - age / window_secs).clamp(0.0, 1.0);
        let index = ((position * buckets as f64) as usize).min(buckets - 1);
        samples[index].push(*value);
    }
    samples.iter().map(|values| average(values)).collect()
}

fn average(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        let scale = values.iter().map(|value| value.abs()).fold(0.0, f64::max);
        if scale == 0.0 {
            return 0.0;
        }
        (values.iter().map(|value| value / scale).sum::<f64>() / values.len() as f64) * scale
    }
}
