use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::model::{Dashboard, DashboardId, RunState};

pub struct Paths {
    root: PathBuf,
}

impl Paths {
    pub fn resolve() -> Self {
        let root = std::env::var_os("QCPERF_TUI_CONFIG")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::current_exe().ok().and_then(|exe| {
                    exe.parent().map(|dir| dir.join("config"))
                })
            })
            .unwrap_or_else(|| PathBuf::from("config"));
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn colors(&self) -> PathBuf {
        self.root.join("colors.toml")
    }

    pub fn dashboards_dir(&self) -> PathBuf {
        self.root.join("dashboards")
    }

    pub fn exports_dir(&self) -> PathBuf {
        self.root.join("exports")
    }

    pub fn snapshots_dir(&self) -> PathBuf {
        self.root.join("snapshots")
    }

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(self.dashboards_dir())?;
        fs::create_dir_all(self.exports_dir())?;
        fs::create_dir_all(self.snapshots_dir())?;
        Ok(())
    }

    pub fn load_dashboards(&self) -> Result<Vec<Dashboard>> {
        let dir = self.dashboards_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut dashboards = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                continue;
            }
            let text = fs::read_to_string(&path)?;
            let mut dashboard: Dashboard =
                toml::from_str(&text).map_err(|err| Error::Toml(err.to_string()))?;
            dashboard.run = RunState::Idle;
            dashboards.push(dashboard);
        }
        dashboards.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(dashboards)
    }

    pub fn save_dashboard(&self, dashboard: &Dashboard) -> Result<()> {
        self.ensure()?;
        let path = self.dashboards_dir().join(format!("{}.toml", dashboard.id));
        let text = toml::to_string_pretty(dashboard).map_err(|err| Error::Toml(err.to_string()))?;
        fs::write(path, text)?;
        Ok(())
    }

    pub fn delete_dashboard(&self, id: DashboardId) -> Result<()> {
        let path = self.dashboards_dir().join(format!("{id}.toml"));
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}
