use std::process::ExitCode;

use qcperf_tui::app::App;
use qcperf_tui::backend::QcPerf;
use qcperf_tui::persist::Paths;

fn main() -> ExitCode {
    let paths = Paths::resolve();
    let app = App::new(QcPerf::init(), paths);
    match app.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}
