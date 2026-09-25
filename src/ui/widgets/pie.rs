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
        if inner.width < 4 || inner.height < 2 || self.slices.is_empty() {
            return;
        }
        let total: f64 = self.slices.iter().map(|slice| slice.value.max(0.0)).sum();
        let legend_width = (inner.width / 2).clamp(10, inner.width.saturating_sub(4));
        let pie = Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width.saturating_sub(legend_width),
            height: inner.height,
        };
        let legend = Rect {
            x: inner.x + pie.width,
            y: inner.y,
            width: inner.width - pie.width,
            height: inner.height,
        };
        draw_disc(pie, buf, self.slices, total);
        for (index, slice) in self.slices.iter().enumerate() {
            if index as u16 >= legend.height {
                break;
            }
            let share = if total > 0.0 {
                slice.value.max(0.0) / total * 100.0
            } else {
                0.0
            };
            let text = format!("{} {share:.1}", truncate(&slice.label, legend.width as usize));
            buf.set_string(
                legend.x,
                legend.y + index as u16,
                text,
                Style::default().fg(slice.color),
            );
        }
    }
}

fn draw_disc(area: Rect, buf: &mut Buffer, slices: &[PieSlice], total: f64) {
    if area.width < 2 || area.height < 2 {
        return;
    }
    let cx = area.width as f64 / 2.0;
    let cy = area.height as f64 / 2.0;
    let radius = cx.min(cy * 2.0) * 0.95;
    for row in 0..area.height {
        for col in 0..area.width {
            let dx = col as f64 + 0.5 - cx;
            let dy = (row as f64 + 0.5 - cy) * 2.0;
            if dx * dx + dy * dy > radius * radius {
                continue;
            }
            let color = if total <= 0.0 {
                slices[0].color
            } else {
                let mut angle = dy.atan2(dx);
                if angle < 0.0 {
                    angle += std::f64::consts::TAU;
                }
                let mut cursor = 0.0;
                let mut chosen = slices[0].color;
                for slice in slices {
                    let sweep = slice.value.max(0.0) / total * std::f64::consts::TAU;
                    if angle <= cursor + sweep {
                        chosen = slice.color;
                        break;
                    }
                    cursor += sweep;
                    chosen = slice.color;
                }
                chosen
            };
            buf.set_string(
                area.x + col,
                area.y + row,
                "█",
                Style::default().fg(color),
            );
        }
    }
}

fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let count = text.chars().count();
    if count <= width {
        text.to_string()
    } else {
        text.chars().take(width.saturating_sub(1)).collect::<String>() + "…"
    }
}
