use std::time::{Duration, SystemTime};

use qcperf_tui::model::{
    ring_capacity, Capability, CapabilityRate, ChartKind, MetricInfo, SampleRing, MAX_RING_POINTS,
};

fn capability() -> Capability {
    Capability {
        backend_id: 3,
        capability_id: 1,
        name: "load".into(),
        metrics: vec![MetricInfo {
            metric_id: 1,
            name: "util".into(),
            description: String::new(),
            unit: "%".into(),
        }],
        sampling_rates_ms: vec![100, 200, 500],
        streaming_rates_ms: vec![100, 500, 1000],
    }
}

#[test]
fn rejects_rate_outside_the_capability_list() {
    let cap = capability();
    let rate = CapabilityRate {
        backend_id: 3,
        capability_id: 1,
        sampling_rate_ms: 150,
        streaming_rate_ms: 1000,
    };
    assert!(rate.validate(&cap).is_err());
}

#[test]
fn rejects_streaming_shorter_than_sampling() {
    let cap = capability();
    let rate = CapabilityRate {
        backend_id: 3,
        capability_id: 1,
        sampling_rate_ms: 500,
        streaming_rate_ms: 100,
    };
    assert!(rate.validate(&cap).is_err());
}

#[test]
fn accepts_listed_rates() {
    let cap = capability();
    let rate = CapabilityRate {
        backend_id: 3,
        capability_id: 1,
        sampling_rate_ms: 200,
        streaming_rate_ms: 1000,
    };
    assert!(rate.validate(&cap).is_ok());
}

#[test]
fn pie_is_the_default_for_percent_and_line_for_mhz() {
    assert_eq!(ChartKind::default_for("%"), ChartKind::Pie);
    assert_eq!(ChartKind::default_for("percent"), ChartKind::Pie);
    assert_eq!(ChartKind::default_for("MHz"), ChartKind::Line);
}

#[test]
fn ring_drops_samples_outside_the_window() {
    let mut ring = SampleRing::new(Duration::from_secs(1), Duration::from_millis(100));
    let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    ring.push(start, 1.0);
    ring.push(start + Duration::from_millis(500), 2.0);
    ring.push(start + Duration::from_secs(2), 3.0);
    let values: Vec<_> = ring.points().iter().map(|(_, value)| *value).collect();
    assert_eq!(values, vec![3.0]);
}

#[test]
fn ring_capacity_is_capped() {
    assert_eq!(
        ring_capacity(Duration::from_secs(10_000), Duration::from_millis(1)),
        MAX_RING_POINTS
    );
}
