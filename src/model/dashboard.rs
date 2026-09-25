use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::capability::Capability;
use crate::model::graph::{default_rate, CapabilityRate, ChartKind, GraphSpec, SeriesColor};
use crate::model::unit::{is_frequency, is_percent};

pub type DashboardId = Uuid;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunState {
    #[default]
    Idle,
    Running,
    Stopped,
}

impl RunState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Running => "Running",
            Self::Stopped => "Stopped",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Dashboard {
    pub id: DashboardId,
    pub name: String,
    pub graphs: Vec<GraphSpec>,
    pub rates: Vec<CapabilityRate>,
    #[serde(skip)]
    pub run: RunState,
    #[serde(default)]
    pub csv_export: bool,
}

impl Dashboard {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            graphs: Vec::new(),
            rates: Vec::new(),
            run: RunState::Idle,
            csv_export: false,
        }
    }

    /// One dashboard covering every connected capability. Chart kind follows
    /// the metric unit. No backend id is special-cased.
    pub fn overview(capabilities: &[Capability]) -> Self {
        let mut dashboard = Self::new("Overview");
        let mut color = 0u8;
        for capability in capabilities {
            let Some(rate) = default_rate(capability) else {
                continue;
            };
            let mut graphs = Vec::new();
            for metric in &capability.metrics {
                let chart = if is_percent(&metric.unit) {
                    ChartKind::Pie
                } else if is_frequency(&metric.unit) {
                    ChartKind::Line
                } else {
                    ChartKind::default_for(&metric.unit)
                };
                graphs.push(GraphSpec {
                    backend_id: capability.backend_id,
                    capability_id: capability.capability_id,
                    metric_id: metric.metric_id,
                    chart,
                    color: SeriesColor::new(color),
                    window: Duration::from_secs(60),
                });
                color = color.wrapping_add(1);
            }
            if !graphs.is_empty() {
                dashboard.rates.push(rate);
                dashboard.graphs.extend(graphs);
            }
        }
        dashboard
    }

    pub fn rate(&self, backend_id: u8, capability_id: u8) -> Option<&CapabilityRate> {
        self.rates
            .iter()
            .find(|rate| rate.backend_id == backend_id && rate.capability_id == capability_id)
    }

    pub fn contains_metric(&self, backend_id: u8, capability_id: u8, metric_id: u16) -> bool {
        self.graphs.iter().any(|graph| {
            graph.backend_id == backend_id
                && graph.capability_id == capability_id
                && graph.metric_id == metric_id
        })
    }
}
