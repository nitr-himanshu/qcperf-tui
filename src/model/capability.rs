use serde::{Deserialize, Serialize};

/// Owned copy of one libqcperf capability. Backend identity is the numeric id
/// the library reports. This crate does not name backends.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Capability {
    pub backend_id: u8,
    pub capability_id: u8,
    pub name: String,
    pub metrics: Vec<MetricInfo>,
    pub sampling_rates_ms: Vec<u16>,
    pub streaming_rates_ms: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MetricInfo {
    pub metric_id: u16,
    pub name: String,
    pub description: String,
    pub unit: String,
}

impl Capability {
    pub fn metric(&self, metric_id: u16) -> Option<&MetricInfo> {
        self.metrics.iter().find(|metric| metric.metric_id == metric_id)
    }

    pub fn label(&self) -> String {
        if self.name.is_empty() {
            format!("backend {} capability {}", self.backend_id, self.capability_id)
        } else {
            format!("{} (backend {})", self.name, self.backend_id)
        }
    }
}

pub fn find<'a>(
    capabilities: &'a [Capability],
    backend_id: u8,
    capability_id: u8,
) -> Option<&'a Capability> {
    capabilities
        .iter()
        .find(|cap| cap.backend_id == backend_id && cap.capability_id == capability_id)
}
