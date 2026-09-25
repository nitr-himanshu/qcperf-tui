use std::io::{self, Stdout};
use std::sync::mpsc::Receiver;
use std::time::{Duration, SystemTime};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::backend::{try_recv, BridgeEvent, QcPerf, SessionTable};
use crate::error::Result;
use crate::export::csv::{self, safe_name};
use crate::export::snapshot;
use crate::model::{lookup_metric, Dashboard, DashboardId, RunState, Sample};
use crate::persist::Paths;
use crate::theme::Theme;
use crate::ui::{self, Editor, EditorEffect};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    List,
    View,
    Edit,
    Help,
}

pub struct App {
    paths: Paths,
    qc: Option<QcPerf>,
    events: Receiver<BridgeEvent>,
    sessions: SessionTable,
    dashboards: Vec<Dashboard>,
    selected: usize,
    screen: Screen,
    editor: Option<Editor>,
    help_return: Screen,
    theme: Theme,
    status: String,
    init_error: Option<String>,
}

impl App {
    pub fn new(qc: Result<(QcPerf, Receiver<BridgeEvent>)>, paths: Paths) -> Self {
        let _ = paths.ensure();
        let theme = Theme::load(&paths.colors());
        let (qc, events, init_error) = match qc {
            Ok((qc, events)) => (Some(qc), events, None),
            Err(err) => {
                let (_tx, rx) = std::sync::mpsc::sync_channel(1);
                (None, rx, Some(err.to_string()))
            }
        };
        let mut dashboards = paths.load_dashboards().unwrap_or_default();
        if dashboards.is_empty() {
            if let Some(qc) = qc.as_ref() {
                let overview = Dashboard::overview(qc.capabilities());
                let _ = paths.save_dashboard(&overview);
                dashboards.push(overview);
            }
        }
        let mut status = init_error.clone().unwrap_or_else(|| {
            "Up/Down select a dashboard, Enter opens it, ? shows help".to_string()
        });
        if let Some(qc) = qc.as_ref() {
            for warning in qc.warnings() {
                status = warning.clone();
            }
        }
        Self {
            paths,
            qc,
            events,
            sessions: SessionTable::new(),
            dashboards,
            selected: 0,
            screen: Screen::List,
            editor: None,
            help_return: Screen::List,
            theme,
            status,
            init_error,
        }
    }

    pub fn run(mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        let result = self.event_loop(&mut terminal);
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        self.shutdown();
        result
    }

    fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    ) -> Result<()> {
        loop {
            terminal.draw(|frame| ui::draw(frame, self))?;
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press && self.handle_key(key) {
                        return Ok(());
                    }
                }
            }
            self.drain_events();
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Char('q') && self.screen != Screen::Edit {
            return true;
        }
        if key.code == KeyCode::Char('?') && self.screen != Screen::Edit {
            self.help_return = self.screen;
            self.screen = Screen::Help;
            return false;
        }
        match self.screen {
            Screen::List => self.key_list(key),
            Screen::View => self.key_view(key),
            Screen::Edit => self.key_edit(key),
            Screen::Help => {
                if key.code == KeyCode::Esc {
                    self.screen = self.help_return;
                }
            }
        }
        false
    }

    fn key_list(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                if self.selected + 1 < self.dashboards.len() {
                    self.selected += 1;
                }
            }
            KeyCode::Enter => {
                if !self.dashboards.is_empty() {
                    self.screen = Screen::View;
                }
            }
            KeyCode::Char('n') => {
                let dashboard = Dashboard::new(format!("Dashboard {}", self.dashboards.len() + 1));
                let _ = self.paths.save_dashboard(&dashboard);
                self.dashboards.push(dashboard);
                self.selected = self.dashboards.len() - 1;
                self.open_editor();
            }
            KeyCode::Char('d') => self.delete_selected(),
            _ => {}
        }
    }

    fn key_view(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.screen = Screen::List,
            KeyCode::Char('e') => self.open_editor(),
            KeyCode::Char('s') => self.start_selected(),
            KeyCode::Char('x') => self.stop_selected(),
            KeyCode::Char('c') => self.export_now(),
            KeyCode::Char('v') => self.toggle_csv_follow(),
            KeyCode::Char('p') => self.snapshot_selected(),
            KeyCode::Tab | KeyCode::Right => self.cycle(1),
            KeyCode::Left => self.cycle(-1),
            _ => {}
        }
    }

    fn key_edit(&mut self, key: KeyEvent) {
        let locked = {
            let Some(editor) = self.editor.as_ref() else {
                self.screen = Screen::View;
                return;
            };
            let cursor = editor.cursor;
            let dashboard_id = editor.dashboard.id;
            let caps = self.qc.as_ref().map(|qc| qc.capabilities()).unwrap_or(&[]);
            let mut rows = Vec::new();
            for cap in caps {
                for _metric in &cap.metrics {
                    rows.push((cap.backend_id, cap.capability_id));
                }
            }
            if rows.is_empty() {
                false
            } else {
                let (backend_id, capability_id) = rows[cursor.min(rows.len() - 1)];
                self.sessions
                    .holders(backend_id, capability_id)
                    .into_iter()
                    .any(|id| id != dashboard_id)
            }
        };
        let capabilities = self.capabilities().to_vec();
        let Some(editor) = self.editor.as_mut() else {
            self.screen = Screen::View;
            return;
        };
        let effect = editor.handle_key(key, &capabilities, locked);
        match effect {
            EditorEffect::None => {}
            EditorEffect::Cancel => {
                self.editor = None;
                self.screen = if self.dashboards.is_empty() {
                    Screen::List
                } else {
                    Screen::View
                };
            }
            EditorEffect::Saved => self.save_editor(),
            EditorEffect::Status(text) => self.status = text,
        }
    }

    fn open_editor(&mut self) {
        let Some(dashboard) = self.dashboards.get(self.selected).cloned() else {
            return;
        };
        self.editor = Some(Editor::open(dashboard));
        self.screen = Screen::Edit;
    }

    fn save_editor(&mut self) {
        let Some(editor) = self.editor.take() else {
            return;
        };
        let mut dashboard = editor.dashboard;
        let was_running = self
            .dashboards
            .iter()
            .find(|existing| existing.id == dashboard.id)
            .map(|existing| existing.run)
            .unwrap_or(RunState::Idle);
        dashboard.run = was_running;
        if was_running == RunState::Running {
            if let Some(qc) = self.qc.as_mut() {
                match self.sessions.reconcile(qc, &dashboard) {
                    Ok(Some(note)) => self.status = note,
                    Ok(None) => self.status = format!("saved {}", dashboard.name),
                    Err(err) => self.status = err.to_string(),
                }
            }
        } else {
            self.status = format!("saved {}", dashboard.name);
        }
        if let Some(slot) = self
            .dashboards
            .iter_mut()
            .find(|existing| existing.id == dashboard.id)
        {
            *slot = dashboard.clone();
        }
        let _ = self.paths.save_dashboard(&dashboard);
        self.screen = Screen::View;
    }

    fn start_selected(&mut self) {
        let Some(index) = self.current_index() else {
            return;
        };
        if self.qc.is_none() {
            self.status = self
                .init_error
                .clone()
                .unwrap_or_else(|| "libqcperf is not initialized".to_string());
            return;
        }
        let dashboard = self.dashboards[index].clone();
        if dashboard.rates.is_empty() {
            self.status = "edit the dashboard and choose at least one metric before starting".to_string();
            return;
        }
        let Some(qc) = self.qc.as_mut() else {
            return;
        };
        match self.sessions.start(qc, &dashboard) {
            Ok(()) => {
                self.dashboards[index].run = RunState::Running;
                self.status = format!("started {}", dashboard.name);
            }
            Err(err) => self.status = err.to_string(),
        }
    }

    fn stop_selected(&mut self) {
        let Some(index) = self.current_index() else {
            return;
        };
        let id = self.dashboards[index].id;
        let Some(qc) = self.qc.as_mut() else {
            return;
        };
        match self.sessions.stop(qc, id) {
            Ok(()) => {
                self.dashboards[index].run = RunState::Stopped;
                self.status = format!("stopped {}", self.dashboards[index].name);
            }
            Err(err) => self.status = err.to_string(),
        }
    }

    fn cycle(&mut self, dir: i32) {
        if self.dashboards.is_empty() {
            return;
        }
        let len = self.dashboards.len() as i32;
        let next = (self.selected as i32 + dir).rem_euclid(len) as usize;
        self.selected = next;
        self.status = format!("showing {}", self.dashboards[self.selected].name);
    }

    fn delete_selected(&mut self) {
        let Some(index) = self.current_index() else {
            return;
        };
        if self.dashboards[index].run == RunState::Running {
            self.status = "stop the dashboard before deleting it".to_string();
            return;
        }
        let id = self.dashboards[index].id;
        let name = self.dashboards[index].name.clone();
        self.dashboards.remove(index);
        if self.selected >= self.dashboards.len() && self.selected > 0 {
            self.selected -= 1;
        }
        let _ = self.paths.delete_dashboard(id);
        self.status = format!("deleted {name}");
    }

    fn export_now(&mut self) {
        let Some(dashboard) = self.selected_dashboard().cloned() else {
            return;
        };
        match self.write_csv(&dashboard, None) {
            Ok(path) => self.status = format!("csv {}", path.display()),
            Err(err) => self.status = err.to_string(),
        }
    }

    fn toggle_csv_follow(&mut self) {
        let Some(dashboard) = self.dashboards.get_mut(self.selected) else {
            return;
        };
        dashboard.csv_export = !dashboard.csv_export;
        let enabled = dashboard.csv_export;
        let name = dashboard.name.clone();
        let _ = self.paths.save_dashboard(dashboard);
        self.status = if enabled {
            format!("{name} will append CSV while it runs")
        } else {
            format!("{name} CSV follow is off")
        };
    }

    fn snapshot_selected(&mut self) {
        let Some(dashboard) = self.selected_dashboard().cloned() else {
            return;
        };
        let id = dashboard.id;
        let points: Vec<_> = dashboard
            .graphs
            .iter()
            .map(|graph| (graph.metric_id, self.sessions.points(id, graph.metric_id)))
            .collect();
        let capabilities = self.capabilities().to_vec();
        match snapshot::write_snapshot(&self.paths, &capabilities, &dashboard, &points) {
            Ok(path) => self.status = format!("snapshot {}", path.display()),
            Err(err) => self.status = err.to_string(),
        }
    }

    fn drain_events(&mut self) {
        let mut batches = Vec::new();
        while let Some(event) = try_recv(&self.events) {
            match event {
                BridgeEvent::Samples(samples) => batches.push(samples),
                BridgeEvent::Message { level, text } => {
                    self.status = format!("{level}: {text}");
                }
            }
        }
        for batch in &batches {
            self.sessions.ingest(batch);
            if let Err(err) = self.append_followed(batch) {
                self.status = err.to_string();
            }
        }
    }

    fn append_followed(&self, batch: &[Sample]) -> Result<()> {
        for dashboard in &self.dashboards {
            if dashboard.run != RunState::Running || !dashboard.csv_export {
                continue;
            }
            let rows: Vec<String> = batch
                .iter()
                .filter(|sample| {
                    dashboard.contains_metric(
                        sample.backend_id,
                        sample.capability_id,
                        sample.metric_id,
                    )
                })
                .map(|sample| {
                    let metric = lookup_metric(self.capabilities(), sample);
                    csv::format_row(
                        &dashboard.name,
                        metric.map(|m| m.name.as_str()).unwrap_or(""),
                        metric.map(|m| m.unit.as_str()).unwrap_or(""),
                        sample,
                    )
                })
                .collect();
            let path = self.csv_path(&dashboard.name);
            csv::append_rows(&path, &rows)?;
        }
        Ok(())
    }

    fn write_csv(&self, dashboard: &Dashboard, only: Option<&[Sample]>) -> Result<std::path::PathBuf> {
        let path = self.csv_path(&dashboard.name);
        let rows = if let Some(batch) = only {
            batch
                .iter()
                .filter(|sample| {
                    dashboard.contains_metric(sample.backend_id, sample.capability_id, sample.metric_id)
                })
                .map(|sample| self.row(dashboard, sample))
                .collect()
        } else {
            dashboard
                .graphs
                .iter()
                .flat_map(|graph| {
                    self.sessions
                        .points(dashboard.id, graph.metric_id)
                        .into_iter()
                        .map(|point| {
                            let sample = Sample {
                                at: point.0,
                                backend_id: graph.backend_id,
                                capability_id: graph.capability_id,
                                metric_id: graph.metric_id,
                                value: point.1,
                            };
                            self.row(dashboard, &sample)
                        })
                })
                .collect()
        };
        csv::append_rows(&path, &rows)?;
        Ok(path)
    }

    fn row(&self, dashboard: &Dashboard, sample: &Sample) -> String {
        let metric = lookup_metric(self.capabilities(), sample);
        csv::format_row(
            &dashboard.name,
            metric.map(|m| m.name.as_str()).unwrap_or(""),
            metric.map(|m| m.unit.as_str()).unwrap_or(""),
            sample,
        )
    }

    fn csv_path(&self, name: &str) -> std::path::PathBuf {
        let day = csv::format_rfc3339(SystemTime::now());
        let date = day.get(..10).unwrap_or("day");
        self.paths
            .exports_dir()
            .join(format!("{}-{date}.csv", safe_name(name)))
    }

    fn current_index(&self) -> Option<usize> {
        if self.dashboards.is_empty() {
            None
        } else {
            Some(self.selected.min(self.dashboards.len() - 1))
        }
    }

    fn shutdown(&mut self) {
        if let Some(qc) = self.qc.as_mut() {
            let _ = self.sessions.stop_all(qc);
        }
        if let Some(qc) = self.qc.take() {
            let _ = qc.shutdown();
        }
    }

    pub fn screen(&self) -> Screen {
        self.screen
    }

    pub fn dashboards(&self) -> &[Dashboard] {
        &self.dashboards
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_dashboard(&self) -> Option<&Dashboard> {
        self.dashboards.get(self.selected.min(self.dashboards.len().saturating_sub(1)))
            .filter(|_| !self.dashboards.is_empty())
    }

    pub fn capabilities(&self) -> &[crate::model::Capability] {
        self.qc.as_ref().map(|qc| qc.capabilities()).unwrap_or(&[])
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn init_error(&self) -> Option<&str> {
        self.init_error.as_deref()
    }

    pub fn editor(&self) -> Option<&Editor> {
        self.editor.as_ref()
    }

    pub fn points(&self, id: DashboardId, metric_id: u16) -> Vec<(SystemTime, f64)> {
        self.sessions.points(id, metric_id)
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if self.qc.is_some() {
            self.shutdown();
        }
    }
}
