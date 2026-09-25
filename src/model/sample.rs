use std::collections::VecDeque;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::model::capability::Capability;

pub const MAX_RING_POINTS: usize = 4096;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    #[serde(with = "system_time_ms")]
    pub at: SystemTime,
    pub backend_id: u8,
    pub capability_id: u8,
    pub metric_id: u16,
    pub value: f64,
}

#[derive(Clone, Debug)]
pub struct SampleRing {
    window: Duration,
    stream_period: Duration,
    max_points: usize,
    points: VecDeque<(SystemTime, f64)>,
}

impl SampleRing {
    pub fn new(window: Duration, stream_period: Duration) -> Self {
        Self {
            window,
            stream_period,
            max_points: ring_capacity(window, stream_period),
            points: VecDeque::new(),
        }
    }

    pub fn set_window(&mut self, window: Duration) {
        self.window = window;
        self.max_points = ring_capacity(window, self.stream_period);
        self.trim();
    }

    pub fn push(&mut self, at: SystemTime, value: f64) {
        self.points.push_back((at, value));
        self.trim();
    }

    pub fn points(&self) -> &VecDeque<(SystemTime, f64)> {
        &self.points
    }

    pub fn latest(&self) -> Option<f64> {
        self.points.back().map(|(_, value)| *value)
    }

    pub fn window(&self) -> Duration {
        self.window
    }

    fn trim(&mut self) {
        while self.points.len() > self.max_points {
            self.points.pop_front();
        }
        let Some((newest, _)) = self.points.back().copied() else {
            return;
        };
        let Some(cutoff) = newest.checked_sub(self.window) else {
            return;
        };
        while self
            .points
            .front()
            .is_some_and(|(at, _)| *at < cutoff)
        {
            self.points.pop_front();
        }
    }
}

pub fn ring_capacity(window: Duration, stream_period: Duration) -> usize {
    let period_ms = stream_period.as_millis().max(1);
    let points = (window.as_millis() / period_ms) as usize;
    points.clamp(1, MAX_RING_POINTS)
}

pub fn lookup_metric<'a>(
    capabilities: &'a [Capability],
    sample: &Sample,
) -> Option<&'a crate::model::capability::MetricInfo> {
    capabilities
        .iter()
        .find(|cap| cap.backend_id == sample.backend_id && cap.capability_id == sample.capability_id)
        .and_then(|cap| cap.metric(sample.metric_id))
}

mod system_time_ms {
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let millis = value
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        serializer.serialize_u64(millis)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(UNIX_EPOCH + Duration::from_millis(millis))
    }
}
