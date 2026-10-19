//! Diagnostics: a log file in the app's log folder, which a GUI build has
//! nowhere else to write, and the problems the window must hear about (the
//! keyboard out of reach, for instance).

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Log file, once the app knows its folder.
static FILE: Mutex<Option<PathBuf>> = Mutex::new(None);
/// Where problems go to reach the window.
static REPORTER: OnceLock<Box<dyn Fn(String) + Send + Sync>> = OnceLock::new();

/// Past this size the log starts over, keeping the previous one as `.old`.
const MAX_LOG_BYTES: u64 = 1_000_000;

pub fn init(dir: PathBuf, reporter: impl Fn(String) + Send + Sync + 'static) {
    let _ = std::fs::create_dir_all(&dir);
    *FILE.lock().unwrap_or_else(|e| e.into_inner()) = Some(dir.join("totalkontrol.log"));
    let _ = REPORTER.set(Box::new(reporter));
}

/// Appends a line to the log, with the date.
pub fn log(line: impl AsRef<str>) {
    let line = format!("{} {}\n", now(), line.as_ref());
    if cfg!(debug_assertions) {
        eprint!("{line}");
    }
    let Some(path) = FILE.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
        return;
    };
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = std::fs::rename(&path, path.with_extension("log.old"));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(line.as_bytes());
    }
}

/// A problem the user should see: logged, and shown in the window.
pub fn report(problem: String) {
    log(&problem);
    if let Some(reporter) = REPORTER.get() {
        reporter(problem);
    }
}

/// The end of the log, at most `max` bytes, for a diagnostics file.
pub fn tail(max: usize) -> String {
    let Some(path) = FILE.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
        return String::new();
    };
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let start = text.len().saturating_sub(max);
    // Starts on a whole line.
    let start = text[start..].find('\n').map_or(start, |i| start + i + 1);
    text[start.min(text.len())..].to_string()
}

/// Year, month, day, hour, minute, second (UTC) of a Unix time.
fn civil(secs: u64) -> (i64, u64, u64, u64, u64, u64) {
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u64;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u64;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day, rest / 3600, rest % 3600 / 60, rest % 60)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `2026-10-19 18:30:05Z`
pub fn now() -> String {
    let (y, mo, d, h, mi, s) = civil(unix_now());
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}Z")
}

/// `2026-10-19-1830`, for file names.
pub fn file_stamp() -> String {
    let (y, mo, d, h, mi, _) = civil(unix_now());
    format!("{y:04}-{mo:02}-{d:02}-{h:02}{mi:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_civil() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        // 2026-10-19 18:30:05 UTC.
        assert_eq!(civil(1_792_434_605), (2026, 10, 19, 18, 30, 5));
        // A leap day.
        assert_eq!(civil(1_709_164_800), (2024, 2, 29, 0, 0, 0));
    }
}
