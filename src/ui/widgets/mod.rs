mod bar_chart;
mod line_chart;
mod pie;

pub use bar_chart::BarSeries;
pub use line_chart::LineSeries;
pub use pie::{PieChart, PieSlice};

pub fn format_value(value: f64) -> String {
    if !value.is_finite() {
        return "—".to_string();
    }
    let magnitude = value.abs();
    if magnitude >= 1_000_000.0 || (magnitude > 0.0 && magnitude < 0.01) {
        format!("{value:.2e}")
    } else {
        format!("{value:.2}")
    }
}
