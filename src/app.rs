use std::io::{self, IsTerminal, Write};
use std::sync::mpsc::Receiver;
use std::time::{Duration, SystemTime};

use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::{Terminal, TerminalOptions, Viewport};

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
        let mut restore = TerminalRestore::arm()?;
        let result = self.drive(&mut restore);
        restore.restore_if_armed();
        self.shutdown();
        result
    }

    fn drive(&mut self, restore: &mut TerminalRestore) -> Result<()> {
        let mut writer = terminal_writer()?;
        enter_screen(&mut writer, restore.alternate)?;
        let area = current_drawing_area();
        let mut terminal = Terminal::with_options(
            CrosstermBackend::new(writer),
            TerminalOptions {
                viewport: Viewport::Fixed(area),
            },
        )?;
        let result = self.event_loop(&mut terminal, area);
        leave_screen(terminal.backend_mut(), restore.alternate)?;
        restore.disarm();
        result
    }

    fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Box<dyn Write + Send>>>,
        mut area: Rect,
    ) -> Result<()> {
        loop {
            area = sync_viewport(terminal, area, None)?;
            terminal.draw(|frame| ui::draw(frame, self))?;
            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    Event::Key(key) => {
                        if key.kind == KeyEventKind::Press && self.handle_key(key) {
                            return Ok(());
                        }
                    }
                    Event::Resize(width, height) => {
                        area = sync_viewport(terminal, area, Some((width, height)))?;
                    }
                    _ => {}
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
        let rows: Vec<String> = if let Some(batch) = only {
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

/// Restores raw mode and the cursor if terminal setup fails before [`App::drive`] finishes.
struct TerminalRestore {
    armed: bool,
    /// Windows keeps the alternate screen. Unix paints the primary screen.
    alternate: bool,
}

impl TerminalRestore {
    fn arm() -> Result<Self> {
        enable_raw_mode()?;
        Ok(Self {
            armed: true,
            alternate: cfg!(windows),
        })
    }

    fn disarm(&mut self) {
        self.armed = false;
    }

    fn restore_if_armed(&mut self) {
        if !self.armed {
            return;
        }
        self.armed = false;
        if let Ok(mut writer) = terminal_writer() {
            let _ = leave_screen(&mut writer, self.alternate);
        } else {
            let _ = disable_raw_mode();
        }
    }
}

impl Drop for TerminalRestore {
    fn drop(&mut self) {
        self.restore_if_armed();
    }
}

fn terminal_writer() -> io::Result<Box<dyn Write + Send>> {
    let stdout = io::stdout();
    if stdout.is_terminal() {
        return Ok(Box::new(stdout));
    }
    #[cfg(unix)]
    {
        let tty = std::fs::OpenOptions::new().write(true).open("/dev/tty")?;
        return Ok(Box::new(tty));
    }
    #[cfg(not(unix))]
    {
        Ok(Box::new(stdout))
    }
}

fn enter_screen(writer: &mut impl Write, alternate: bool) -> io::Result<()> {
    if alternate {
        execute!(writer, EnterAlternateScreen)?;
    } else {
        execute!(writer, Clear(ClearType::All))?;
    }
    Ok(())
}

fn leave_screen(writer: &mut impl Write, alternate: bool) -> io::Result<()> {
    let drawn = if alternate {
        execute!(writer, LeaveAlternateScreen, Show)
    } else {
        execute!(writer, Show)
    };
    let raw = disable_raw_mode();
    drawn?;
    raw
}

fn current_drawing_area() -> Rect {
    let measured = crossterm::terminal::size().unwrap_or((0, 0));
    drawing_area(measured, env_u16("COLUMNS"), env_u16("LINES"))
}

fn sync_viewport(
    terminal: &mut Terminal<CrosstermBackend<Box<dyn Write + Send>>>,
    current: Rect,
    reported: Option<(u16, u16)>,
) -> Result<Rect> {
    let measured = match reported {
        Some(size) => size,
        None => crossterm::terminal::size().unwrap_or((0, 0)),
    };
    let Some(next) = positive_area(measured) else {
        return Ok(current);
    };
    if next != current {
        terminal.resize(next)?;
    }
    Ok(next)
}

fn positive_area(measured: (u16, u16)) -> Option<Rect> {
    if measured.0 > 0 && measured.1 > 0 {
        Some(Rect::new(0, 0, measured.0, measured.1))
    } else {
        None
    }
}

/// Picks a non-zero viewport. A zero ioctl side uses `COLUMNS` / `LINES` when
/// those are positive, then 80×24.
fn drawing_area(measured: (u16, u16), columns: Option<u16>, lines: Option<u16>) -> Rect {
    let width = if measured.0 > 0 {
        measured.0
    } else {
        columns.filter(|value| *value > 0).unwrap_or(80)
    };
    let height = if measured.1 > 0 {
        measured.1
    } else {
        lines.filter(|value| *value > 0).unwrap_or(24)
    };
    Rect::new(0, 0, width, height)
}

fn env_u16(name: &str) -> Option<u16> {
    let value = std::env::var(name).ok()?;
    let parsed = value.parse::<u16>().ok()?;
    (parsed > 0).then_some(parsed)
}

#[cfg(test)]
mod tests {
    use super::drawing_area;
    use ratatui::layout::Rect;

    #[test]
    fn zero_size_falls_back_to_80x24() {
        assert_eq!(drawing_area((0, 0), None, None), Rect::new(0, 0, 80, 24));
    }

    #[test]
    fn zero_size_uses_positive_env_dimensions() {
        assert_eq!(
            drawing_area((0, 0), Some(100), Some(50)),
            Rect::new(0, 0, 100, 50)
        );
    }

    #[test]
    fn non_positive_env_dimensions_are_ignored() {
        assert_eq!(drawing_area((0, 0), Some(0), None), Rect::new(0, 0, 80, 24));
    }

    #[test]
    fn measured_size_is_kept() {
        assert_eq!(
            drawing_area((120, 40), Some(80), Some(24)),
            Rect::new(0, 0, 120, 40)
        );
    }

    #[test]
    fn only_the_missing_side_falls_back() {
        assert_eq!(
            drawing_area((0, 40), Some(100), Some(10)),
            Rect::new(0, 0, 100, 40)
        );
    }
}
