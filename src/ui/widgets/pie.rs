use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Widget};

pub struct PieSlice {
    pub label: String,
    pub value: f64,
    pub color: Color,
}

pub struct PieChart<'a> {
    pub title: &'a str,
    pub slices: &'a [PieSlice],
}

impl Widget for PieChart<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().borders(Borders::ALL).title(self.title);
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.width < 2 || inner.height < 1 || self.slices.is_empty() {
            return;
        }

        let max_value = self
            .slices
            .iter()
            .map(|slice| slice.value)
            .filter(|value| value.is_finite() && *value > 0.0)
            .fold(0.0, f64::max);
        let total_scaled: f64 = if max_value > 0.0 {
            self.slices
                .iter()
                .map(|slice| positive_weight(slice.value) / max_value)
                .sum()
        } else {
            0.0
        };

        let (pie, legend) = if inner.width >= 24 {
            let legend_width = (inner.width / 3).clamp(12, inner.width - 12);
            let pie_width = inner.width - legend_width;
            (
                Rect::new(inner.x, inner.y, pie_width, inner.height),
                Some(Rect::new(
                    inner.x + pie_width,
                    inner.y,
                    legend_width,
                    inner.height,
                )),
            )
        } else if inner.width >= 8 && inner.height >= 7 {
            let pie_height = (inner.height.saturating_mul(2) / 3).max(3);
            (
                Rect::new(inner.x, inner.y, inner.width, pie_height),
                Some(Rect::new(
                    inner.x,
                    inner.y + pie_height,
                    inner.width,
                    inner.height - pie_height,
                )),
            )
        } else {
            (inner, None)
        };

        draw_disc(pie, buf, self.slices, max_value, total_scaled);
        if let Some(legend) = legend {
            draw_legend(legend, buf, self.slices, max_value, total_scaled);
        }
    }
}

fn draw_legend(
    area: Rect,
    buf: &mut Buffer,
    slices: &[PieSlice],
    max_value: f64,
    total_scaled: f64,
) {
    for (index, slice) in slices.iter().enumerate() {
        if index >= area.height as usize {
            break;
        }
        let share = if max_value > 0.0 {
            positive_weight(slice.value) / max_value / total_scaled * 100.0
        } else {
            0.0
        };
        let text = truncate(
            &format!("{share:>5.1}% {}", slice.label),
            area.width as usize,
        );
        buf.set_string(
            area.x,
            area.y + index as u16,
            text,
            Style::default().fg(slice.color),
        );
    }
}

/// A terminal cell is roughly twice as tall as it is wide. Drawing two
/// independently colored half-block pixels per cell keeps the pie circular
/// and gives sector edges twice the vertical resolution.
fn draw_disc(area: Rect, buf: &mut Buffer, slices: &[PieSlice], max_value: f64, total_scaled: f64) {
    if area.width < 2 || area.height < 1 {
        return;
    }
    let cx = area.width as f64 / 2.0;
    let cy = area.height as f64;
    let radius = cx.min(cy) * 0.96;

    for row in 0..area.height {
        for col in 0..area.width {
            let dx = col as f64 + 0.5 - cx;
            let top_y = row as f64 * 2.0 + 0.5;
            let bottom_y = top_y + 1.0;
            let top = disc_color(dx, top_y, cx, cy, radius, slices, max_value, total_scaled);
            let bottom = disc_color(
                dx,
                bottom_y,
                cx,
                cy,
                radius,
                slices,
                max_value,
                total_scaled,
            );
            let cell = buf.get_mut(area.x + col, area.y + row);
            match (top, bottom) {
                (Color::Reset, Color::Reset) => {
                    cell.set_char(' ').set_fg(Color::Reset).set_bg(Color::Reset);
                }
                (Color::Reset, bottom) => {
                    cell.set_char('▄').set_fg(bottom).set_bg(Color::Reset);
                }
                (top, Color::Reset) => {
                    cell.set_char('▀').set_fg(top).set_bg(Color::Reset);
                }
                (top, bottom) => {
                    cell.set_char('▀').set_fg(top).set_bg(bottom);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn disc_color(
    dx: f64,
    y: f64,
    cx: f64,
    cy: f64,
    radius: f64,
    slices: &[PieSlice],
    max_value: f64,
    total_scaled: f64,
) -> Color {
    let dy = y - cy;
    if dx * dx + dy * dy > radius * radius {
        return Color::Reset;
    }
    if max_value <= 0.0 || total_scaled <= 0.0 {
        return Color::DarkGray;
    }

    let mut angle = dy.atan2(dx);
    if angle < 0.0 {
        angle += std::f64::consts::TAU;
    }
    let target = angle / std::f64::consts::TAU;
    let mut cursor = 0.0;
    for slice in slices {
        let weight = positive_weight(slice.value) / max_value / total_scaled;
        if weight == 0.0 {
            continue;
        }
        cursor += weight;
        if target < cursor {
            return slice.color;
        }
    }
    slices
        .iter()
        .rev()
        .find(|slice| positive_weight(slice.value) > 0.0)
        .map(|slice| slice.color)
        .unwrap_or(Color::DarkGray)
}

fn positive_weight(value: f64) -> f64 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let count = text.chars().count();
    if count <= width {
        text.to_string()
    } else if width == 1 {
        "…".to_string()
    } else {
        text.chars().take(width - 1).collect::<String>() + "…"
    }
}
