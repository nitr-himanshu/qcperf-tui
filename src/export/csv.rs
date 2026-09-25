use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::Result;
use crate::model::Sample;

pub const HEADER: &str =
    "timestamp_rfc3339,dashboard,backend_id,capability_id,metric_id,metric_name,value,unit";

pub fn format_row(
    dashboard: &str,
    metric_name: &str,
    unit: &str,
    sample: &Sample,
) -> String {
    format!(
        "{},{},{},{},{},{},{},{}",
        format_rfc3339(sample.at),
        escape(dashboard),
        sample.backend_id,
        sample.capability_id,
        sample.metric_id,
        escape(metric_name),
        sample.value,
        escape(unit)
    )
}

pub fn append_rows(path: &Path, rows: &[String]) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let new_file = !path.exists();
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    if new_file {
        writeln!(file, "{HEADER}")?;
    }
    for row in rows {
        writeln!(file, "{row}")?;
    }
    Ok(())
}

pub fn format_rfc3339(time: SystemTime) -> String {
    let duration = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs() as i64;
    let millis = duration.subsec_millis();
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = tod / 3600;
    let minute = (tod % 3600) / 60;
    let second = tod % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

pub fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "dashboard".to_string()
    } else {
        cleaned
    }
}

fn escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (year as i32, month as u32, day as u32)
}
