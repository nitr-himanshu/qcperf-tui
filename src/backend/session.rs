use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use crate::backend::QcPerf;
use crate::error::{Error, Result};
use crate::model::{
    CapabilityRate, Dashboard, DashboardId, Sample, SampleRing,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SessionKey {
    backend_id: u8,
    capability_id: u8,
    sampling_rate_ms: u16,
    streaming_rate_ms: u16,
}

impl SessionKey {
    fn from_rate(rate: &CapabilityRate) -> Self {
        Self {
            backend_id: rate.backend_id,
            capability_id: rate.capability_id,
            sampling_rate_ms: rate.sampling_rate_ms,
            streaming_rate_ms: rate.streaming_rate_ms,
        }
    }

    fn rate(self) -> CapabilityRate {
        CapabilityRate {
            backend_id: self.backend_id,
            capability_id: self.capability_id,
            sampling_rate_ms: self.sampling_rate_ms,
            streaming_rate_ms: self.streaming_rate_ms,
        }
    }

    fn period(self) -> Duration {
        Duration::from_millis(self.streaming_rate_ms as u64)
    }
}

struct Subscribers {
    dashboards: Vec<DashboardId>,
    rings: HashMap<(DashboardId, u16), SampleRing>,
}

pub struct SessionTable {
    active: HashMap<SessionKey, Subscribers>,
}

impl SessionTable {
    pub fn new() -> Self {
        Self {
            active: HashMap::new(),
        }
    }

    pub fn start(&mut self, qc: &mut QcPerf, dashboard: &Dashboard) -> Result<()> {
        for rate in &dashboard.rates {
            if let Some(live) = self.live_rate(rate.backend_id, rate.capability_id) {
                if live.sampling_rate_ms != rate.sampling_rate_ms
                    || live.streaming_rate_ms != rate.streaming_rate_ms
                {
                    return Err(Error::message(format!(
                        "backend {} capability {} is already streaming at sample {} ms / stream {} ms",
                        rate.backend_id,
                        rate.capability_id,
                        live.sampling_rate_ms,
                        live.streaming_rate_ms
                    )));
                }
            }
        }

        for rate in &dashboard.rates {
            let key = SessionKey::from_rate(rate);
            if !self.active.contains_key(&key) {
                qc.start(rate)?;
                self.active.insert(
                    key,
                    Subscribers {
                        dashboards: Vec::new(),
                        rings: HashMap::new(),
                    },
                );
            }
            let slot = self.active.get_mut(&key).expect("session inserted");
            if !slot.dashboards.contains(&dashboard.id) {
                slot.dashboards.push(dashboard.id);
            }
            for graph in dashboard.graphs.iter().filter(|graph| {
                graph.backend_id == rate.backend_id && graph.capability_id == rate.capability_id
            }) {
                slot.rings.entry((dashboard.id, graph.metric_id)).or_insert_with(|| {
                    SampleRing::new(graph.window, key.period())
                });
            }
        }
        Ok(())
    }

    pub fn stop(&mut self, qc: &mut QcPerf, dashboard_id: DashboardId) -> Result<()> {
        let keys: Vec<SessionKey> = self
            .active
            .iter()
            .filter(|(_, slot)| slot.dashboards.contains(&dashboard_id))
            .map(|(key, _)| *key)
            .collect();
        for key in keys {
            let Some(slot) = self.active.get_mut(&key) else {
                continue;
            };
            slot.dashboards.retain(|id| *id != dashboard_id);
            slot.rings.retain(|(id, _), _| *id != dashboard_id);
            if slot.dashboards.is_empty() {
                qc.stop(&key.rate())?;
                self.active.remove(&key);
            }
        }
        Ok(())
    }

    pub fn stop_all(&mut self, qc: &mut QcPerf) -> Result<()> {
        let keys: Vec<SessionKey> = self.active.keys().copied().collect();
        for key in keys {
            qc.stop(&key.rate())?;
        }
        self.active.clear();
        Ok(())
    }

    /// Apply an edit to a dashboard that is already running.
    ///
    /// Color and window changes update the existing rings. A different rate is
    /// left on the dashboard and takes effect the next time that capability is
    /// started, so a shared session is not restarted underneath another dashboard.
    pub fn reconcile(&mut self, qc: &mut QcPerf, dashboard: &Dashboard) -> Result<Option<String>> {
        let mut note = None;
        let subscribed: Vec<SessionKey> = self
            .active
            .iter()
            .filter(|(_, slot)| slot.dashboards.contains(&dashboard.id))
            .map(|(key, _)| *key)
            .collect();

        for key in subscribed {
            let still = dashboard.rate(key.backend_id, key.capability_id);
            let same_rate = still.is_some_and(|rate| {
                rate.sampling_rate_ms == key.sampling_rate_ms
                    && rate.streaming_rate_ms == key.streaming_rate_ms
            });
            if still.is_none() {
                self.detach(qc, key, dashboard.id)?;
            } else if !same_rate {
                let others = self
                    .active
                    .get(&key)
                    .is_some_and(|slot| slot.dashboards.iter().any(|id| *id != dashboard.id));
                if others {
                    note = Some(format!(
                        "backend {} capability {} stays at sample {} ms / stream {} ms until every dashboard stops it",
                        key.backend_id,
                        key.capability_id,
                        key.sampling_rate_ms,
                        key.streaming_rate_ms
                    ));
                }
                self.sync_rings(key, dashboard);
            } else {
                self.sync_rings(key, dashboard);
            }
        }

        for rate in &dashboard.rates {
            if self.subscription_key(dashboard.id, rate.backend_id, rate.capability_id).is_some() {
                continue;
            }
            if let Some(live) = self.live_rate(rate.backend_id, rate.capability_id) {
                if live.sampling_rate_ms != rate.sampling_rate_ms
                    || live.streaming_rate_ms != rate.streaming_rate_ms
                {
                    note = Some(format!(
                        "backend {} capability {} is already streaming at sample {} ms / stream {} ms",
                        rate.backend_id, rate.capability_id, live.sampling_rate_ms, live.streaming_rate_ms
                    ));
                    continue;
                }
            }
            let key = SessionKey::from_rate(rate);
            if !self.active.contains_key(&key) {
                qc.start(rate)?;
                self.active.insert(
                    key,
                    Subscribers {
                        dashboards: Vec::new(),
                        rings: HashMap::new(),
                    },
                );
            }
            let slot = self.active.get_mut(&key).expect("session inserted");
            if !slot.dashboards.contains(&dashboard.id) {
                slot.dashboards.push(dashboard.id);
            }
            self.sync_rings(key, dashboard);
        }
        Ok(note)
    }

    pub fn ingest(&mut self, samples: &[Sample]) {
        for sample in samples {
            let Some(key) = self
                .active
                .keys()
                .copied()
                .find(|key| {
                    key.backend_id == sample.backend_id && key.capability_id == sample.capability_id
                })
            else {
                continue;
            };
            let Some(slot) = self.active.get_mut(&key) else {
                continue;
            };
            for ((dashboard_id, metric_id), ring) in &mut slot.rings {
                if *metric_id == sample.metric_id {
                    let _ = dashboard_id;
                    ring.push(sample.at, sample.value);
                }
            }
        }
    }

    pub fn ring(&self, dashboard_id: DashboardId, metric_id: u16) -> Option<&SampleRing> {
        self.active.values().find_map(|slot| slot.rings.get(&(dashboard_id, metric_id)))
    }

    pub fn points(&self, dashboard_id: DashboardId, metric_id: u16) -> Vec<(SystemTime, f64)> {
        self.ring(dashboard_id, metric_id)
            .map(|ring| ring.points().iter().copied().collect())
            .unwrap_or_default()
    }

    pub fn is_running(&self, dashboard_id: DashboardId) -> bool {
        self.active
            .values()
            .any(|slot| slot.dashboards.contains(&dashboard_id))
    }

    pub fn holders(&self, backend_id: u8, capability_id: u8) -> Vec<DashboardId> {
        self.active
            .iter()
            .filter(|(key, _)| key.backend_id == backend_id && key.capability_id == capability_id)
            .flat_map(|(_, slot)| slot.dashboards.iter().copied())
            .collect()
    }

    fn live_rate(&self, backend_id: u8, capability_id: u8) -> Option<CapabilityRate> {
        self.active.keys().find_map(|key| {
            (key.backend_id == backend_id && key.capability_id == capability_id).then(|| key.rate())
        })
    }

    fn subscription_key(
        &self,
        dashboard_id: DashboardId,
        backend_id: u8,
        capability_id: u8,
    ) -> Option<SessionKey> {
        self.active.iter().find_map(|(key, slot)| {
            (key.backend_id == backend_id
                && key.capability_id == capability_id
                && slot.dashboards.contains(&dashboard_id))
            .then_some(*key)
        })
    }

    fn sync_rings(&mut self, key: SessionKey, dashboard: &Dashboard) {
        let Some(slot) = self.active.get_mut(&key) else {
            return;
        };
        let wanted: Vec<_> = dashboard
            .graphs
            .iter()
            .filter(|graph| {
                graph.backend_id == key.backend_id && graph.capability_id == key.capability_id
            })
            .map(|graph| (graph.metric_id, graph.window))
            .collect();
        slot.rings
            .retain(|(id, metric), _| *id != dashboard.id || wanted.iter().any(|(m, _)| m == metric));
        for (metric_id, window) in wanted {
            match slot.rings.get_mut(&(dashboard.id, metric_id)) {
                Some(ring) => ring.set_window(window),
                None => {
                    slot.rings.insert(
                        (dashboard.id, metric_id),
                        SampleRing::new(window, key.period()),
                    );
                }
            }
        }
    }

    fn detach(&mut self, qc: &mut QcPerf, key: SessionKey, dashboard_id: DashboardId) -> Result<()> {
        let Some(slot) = self.active.get_mut(&key) else {
            return Ok(());
        };
        slot.dashboards.retain(|id| *id != dashboard_id);
        slot.rings.retain(|(id, _), _| *id != dashboard_id);
        if slot.dashboards.is_empty() {
            qc.stop(&key.rate())?;
            self.active.remove(&key);
        }
        Ok(())
    }
}

impl Default for SessionTable {
    fn default() -> Self {
        Self::new()
    }
}
