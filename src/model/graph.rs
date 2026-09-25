use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::model::capability::Capability;
use crate::model::unit::is_percent;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChartKind {
    Pie,
    Line,
    Bar,
}

impl ChartKind {
    pub fn default_for(unit: &str) -> Self {
        if is_percent(unit) { Self::Pie } else { Self::Line }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Pie => Self::Line,
            Self::Line => Self::Bar,
            Self::Bar => Self::Pie,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Pie => "pie",
            Self::Line => "line",
            Self::Bar => "bar",
        }
    }
}

/// Palette index. The theme resolves it to an RGB color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeriesColor {
    pub index: u8,
}

impl SeriesColor {
    pub fn new(index: u8) -> Self {
        Self { index }
    }

    pub fn next(self) -> Self {
        Self {
            index: self.index.wrapping_add(1),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphSpec {
    pub backend_id: u8,
    pub capability_id: u8,
    pub metric_id: u16,
    pub chart: ChartKind,
    pub color: SeriesColor,
    #[serde(with = "duration_ms")]
    pub window: Duration,
}

/// Sampling and streaming apply to a whole capability, not to one graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityRate {
    pub backend_id: u8,
    pub capability_id: u8,
    pub sampling_rate_ms: u16,
    pub streaming_rate_ms: u16,
}

impl CapabilityRate {
    pub fn validate(&self, capability: &Capability) -> Result<()> {
        if self.backend_id != capability.backend_id || self.capability_id != capability.capability_id
        {
            return Err(Error::message(
                "rate does not belong to the selected capability",
            ));
        }
        if !capability
            .sampling_rates_ms
            .contains(&self.sampling_rate_ms)
        {
            return Err(Error::message(format!(
                "sampling rate {} ms is not offered by {}",
                self.sampling_rate_ms,
                capability.label()
            )));
        }
        if !capability
            .streaming_rates_ms
            .contains(&self.streaming_rate_ms)
        {
            return Err(Error::message(format!(
                "streaming rate {} ms is not offered by {}",
                self.streaming_rate_ms,
                capability.label()
            )));
        }
        if self.streaming_rate_ms < self.sampling_rate_ms {
            return Err(Error::message(format!(
                "streaming rate {} ms is shorter than sampling rate {} ms",
                self.streaming_rate_ms, self.sampling_rate_ms
            )));
        }
        Ok(())
    }
}

pub fn default_rate(capability: &Capability) -> Option<CapabilityRate> {
    let sampling_rate_ms = *capability.sampling_rates_ms.first()?;
    let streaming_rate_ms = capability
        .streaming_rates_ms
        .iter()
        .copied()
        .find(|rate| *rate >= sampling_rate_ms)
        .or_else(|| capability.streaming_rates_ms.last().copied())?;
    Some(CapabilityRate {
        backend_id: capability.backend_id,
        capability_id: capability.capability_id,
        sampling_rate_ms,
        streaming_rate_ms,
    })
}

pub const WINDOW_PRESETS_SECS: [u64; 4] = [15, 30, 60, 300];

pub fn next_window(current: Duration) -> Duration {
    let secs = current.as_secs();
    let next = WINDOW_PRESETS_SECS
        .iter()
        .find(|preset| **preset > secs)
        .copied()
        .unwrap_or(WINDOW_PRESETS_SECS[0]);
    Duration::from_secs(next)
}

mod duration_ms {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(value.as_millis() as u64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}
