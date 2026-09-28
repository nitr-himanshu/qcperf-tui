use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Level {
    Fatal = 1,
    Error = 2,
    Warning = 3,
    Info = 4,
    Debug = 5,
}

impl Level {
    pub fn parse(value: &str) -> Self {
        match value {
            "error" => Self::Error,
            "warning" => Self::Warning,
            "info" => Self::Info,
            "debug" => Self::Debug,
            _ => Self::Debug,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Fatal => "FATAL",
            Self::Error => "ERROR",
            Self::Warning => "WARNING",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Verbosity(u8);

impl Verbosity {
    pub const fn new(level: u8) -> Self {
        Self(if level > Level::Debug as u8 {
            Level::Debug as u8
        } else {
            level
        })
    }

    pub const fn enabled(self, level: Level) -> bool {
        self.0 >= level as u8
    }

    pub fn parse_args<I, S>(args: I) -> Result<Option<Self>, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let mut count = 0u8;
        for arg in args {
            let arg = arg.as_ref().to_string_lossy();
            if arg == "--help" || arg == "-h" {
                return Ok(None);
            }
            if arg == "--verbose" {
                count = count.saturating_add(1);
            } else if arg.starts_with('-') && arg.len() > 1 && arg[1..].bytes().all(|b| b == b'v') {
                count = count.saturating_add((arg.len() - 1).min(u8::MAX as usize) as u8);
            } else {
                return Err(format!("unknown argument: {arg}"));
            }
        }
        Ok(Some(Self::new(count)))
    }
}

pub struct VerboseLog {
    verbosity: Verbosity,
    file: Option<fs::File>,
}

impl VerboseLog {
    pub fn open(verbosity: Verbosity, path: &Path) -> io::Result<Self> {
        let file = if verbosity.0 == 0 {
            None
        } else {
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)?;
            }
            Some(OpenOptions::new().create(true).append(true).open(path)?)
        };
        Ok(Self { verbosity, file })
    }

    pub fn disabled() -> Self {
        Self {
            verbosity: Verbosity::default(),
            file: None,
        }
    }

    pub fn enabled(&self, level: Level) -> bool {
        self.verbosity.enabled(level)
    }

    pub fn record(&mut self, level: Level, message: &str) {
        if !self.verbosity.enabled(level) {
            return;
        }
        let Some(file) = self.file.as_mut() else {
            return;
        };
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        let _ = writeln!(file, "{timestamp} [{}] {message}", level.label());
        let _ = file.flush();
    }
}

pub fn usage(program: &str) -> String {
    format!(
        "Usage: {program} [-v... | --verbose]\n\
         Repeat -v to increase logging detail:\n\
           -v       FATAL only\n\
           -vv      ERROR and FATAL\n\
           -vvv     WARNING, ERROR, and FATAL\n\
           -vvvv    INFO and more severe\n\
           -vvvvv   DEBUG and more severe\n\
         Logs are appended to the application's log file."
    )
}
