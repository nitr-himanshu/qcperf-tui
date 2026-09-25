use std::fs;
use std::time::SystemTime;

use serde::Serialize;

use crate::error::{Error, Result};
use crate::export::csv::{format_rfc3339, safe_name};
use crate::model::{Capability, Dashboard};
use crate::persist::Paths;

#[derive(Serialize)]
struct SnapshotFile {
    name: String,
    run: String,
    graphs: Vec<GraphSnap>,
    rates: Vec<RateSnap>,
    series: Vec<SeriesSnap>,
}

#[derive(Serialize)]
struct GraphSnap {
    backend_id: u8,
    capability_id: u8,
    metric_id: u16,
    chart: String,
    window_ms: u64,
}

#[derive(Serialize)]
struct RateSnap {
    backend_id: u8,
    capability_id: u8,
    sampling_rate_ms: u16,
    streaming_rate_ms: u16,
}

#[derive(Serialize)]
struct SeriesSnap {
    metric_id: u16,
    metric_name: String,
    unit: String,
    points: Vec<PointSnap>,
}

#[derive(Serialize)]
struct PointSnap {
    timestamp_rfc3339: String,
    value: f64,
}

pub fn write_snapshot(
    paths: &Paths,
    capabilities: &[Capability],
    dashboard: &Dashboard,
    points: &[(u16, Vec<(SystemTime, f64)>)],
) -> Result<std::path::PathBuf> {
    let series = dashboard
        .graphs
        .iter()
        .map(|graph| {
            let metric = capabilities.iter().find_map(|cap| {
                (cap.backend_id == graph.backend_id && cap.capability_id == graph.capability_id)
                    .then(|| cap.metric(graph.metric_id))
                    .flatten()
            });
            SeriesSnap {
                metric_id: graph.metric_id,
                metric_name: metric.map(|m| m.name.clone()).unwrap_or_default(),
                unit: metric.map(|m| m.unit.clone()).unwrap_or_default(),
                points: points
                    .iter()
                    .find(|(metric_id, _)| *metric_id == graph.metric_id)
                    .map(|(_, samples)| {
                        samples
                            .iter()
                            .map(|(at, value)| PointSnap {
                                timestamp_rfc3339: format_rfc3339(*at),
                                value: *value,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        })
        .collect();
    let file = SnapshotFile {
        name: dashboard.name.clone(),
        run: dashboard.run.label().to_string(),
        graphs: dashboard
            .graphs
            .iter()
            .map(|graph| GraphSnap {
                backend_id: graph.backend_id,
                capability_id: graph.capability_id,
                metric_id: graph.metric_id,
                chart: graph.chart.label().to_string(),
                window_ms: graph.window.as_millis() as u64,
            })
            .collect(),
        rates: dashboard
            .rates
            .iter()
            .map(|rate| RateSnap {
                backend_id: rate.backend_id,
                capability_id: rate.capability_id,
                sampling_rate_ms: rate.sampling_rate_ms,
                streaming_rate_ms: rate.streaming_rate_ms,
            })
            .collect(),
        series,
    };
    let stamp = safe_name(&format_rfc3339(SystemTime::now()));
    let path = paths
        .snapshots_dir()
        .join(format!("{}-{stamp}.json", safe_name(&dashboard.name)));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(&file).map_err(|err| Error::Json(err.to_string()))?;
    fs::write(&path, text)?;
    Ok(path)
}
