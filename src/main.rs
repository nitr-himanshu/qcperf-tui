use std::process::ExitCode;

use qcperf_tui::app::App;
use qcperf_tui::backend::QcPerf;
use qcperf_tui::logging::{usage, Level, VerboseLog, Verbosity};
use qcperf_tui::persist::Paths;

fn main() -> ExitCode {
    let mut args = std::env::args_os();
    let program = args
        .next()
        .unwrap_or_else(|| "qcperf-tui".into())
        .to_string_lossy()
        .into_owned();
    let verbosity = match Verbosity::parse_args(args) {
        Ok(Some(verbosity)) => verbosity,
        Ok(None) => {
            println!("{}", usage(&program));
            return ExitCode::SUCCESS;
        }
        Err(err) => {
            eprintln!("{err}\n\n{}", usage(&program));
            return ExitCode::from(2);
        }
    };

    let paths = Paths::resolve();
    let log_path = paths.log_file();
    let mut log = match VerboseLog::open(verbosity, &log_path) {
        Ok(log) => log,
        Err(err) => {
            eprintln!("could not open log file {}: {err}", log_path.display());
            return ExitCode::from(2);
        }
    };
    log.record(Level::Debug, "process entry reached");
    log.record(Level::Info, "initializing libqcperf");
    let qc = QcPerf::init_with_log(&mut log);
    let app = App::with_log(qc, paths, log);
    match app.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("FATAL: {err}");
            ExitCode::from(1)
        }
    }
}
