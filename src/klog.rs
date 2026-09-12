//! ============================================================================
//! Structured Kernel Logging (klog)
//! ============================================================================
//!
//! Provides a lightweight level-based logging facility for the kernel,
//! replacing ad-hoc `serial_println!` calls with a consistent line format:
//!
//!   [INFO] [scheduler] [#tick 00042] message text
//!
//! Each line carries the severity level, a subsystem tag, and a PIT tick
//! timestamp. Messages below the configured global minimum level are dropped,
//! allowing coarse runtime filtering (e.g. `klog_set_level(Debug)`).

use core::fmt;
use core::sync::atomic::{AtomicU8, Ordering};

/// Log severity levels, ordered from most verbose (Debug) to most severe (Error).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum LogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        };
        f.write_str(s)
    }
}

/// Global minimum log level. Messages below this threshold are discarded.
static MIN_LEVEL: AtomicU8 = AtomicU8::new(LogLevel::Info as u8);

/// Sets the global minimum log level. Messages below it are dropped.
/// Not yet wired into a console command; kept as the runtime filtering hook.
#[allow(dead_code)]
pub fn set_level(level: LogLevel) {
    MIN_LEVEL.store(level as u8, Ordering::Relaxed);
}

/// Returns the current global minimum log level.
pub fn min_level() -> LogLevel {
    match MIN_LEVEL.load(Ordering::Relaxed) {
        0 => LogLevel::Debug,
        1 => LogLevel::Info,
        2 => LogLevel::Warn,
        _ => LogLevel::Error,
    }
}

/// Internal implementation of the `klog!` macro. Do not call directly.
///
/// Renders `[LEVEL] [subsystem] [#tick] message` on the serial console,
/// prefixed with the current PIT tick count as a lightweight timestamp.
#[doc(hidden)]
pub fn log(level: LogLevel, subsystem: &str, args: fmt::Arguments) {
    if (level as u8) < (min_level() as u8) {
        return;
    }
    let tick = crate::arch::idt::ticks();
    crate::serial_println!("[{}] [{}] [#{:05}] {}", level, subsystem, tick, args);
}

/// Structured kernel log macro.
///
/// # Usage
/// ```ignore
/// klog!(Info, "scheduler", "reaped {} dead task(s)", n);
/// klog!(Warn, "syscall", "unknown syscall #{}", nr);
/// ```
#[macro_export]
macro_rules! klog {
    ($level:ident, $subsystem:expr, $($arg:tt)*) => {
        $crate::klog::log($crate::klog::LogLevel::$level, $subsystem, format_args!($($arg)*))
    };
}