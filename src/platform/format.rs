//! Pure, OS-agnostic formatting and parsing helpers shared by the scheduler/notifier backends.
//!
//! Nothing here performs IO, so it is coverage-on and unit-tested in this file — even the helpers
//! that only a single platform backend *uses* (the Windows XML escaper, the launchd calendar, the
//! AppleScript/VBScript quoters). Keeping the pure logic out of the cfg-gated, IO-heavy backend
//! files (`systemd.rs`, `schtasks.rs`, `launchd.rs`, `notify.rs`) is exactly what lets it be tested
//! on any host, including the Linux coverage job where `schtasks.rs`/`launchd.rs` never compile.
//!
//! Each helper is gated to the platform that uses it plus `test`, so it is compiled and exercised
//! by the unit tests below on every host (keeping coverage honest), while never tripping the
//! `dead_code` lint in a non-test build of a platform that doesn't use it.

use jiff::{Timestamp, tz::TimeZone};

/// systemd `OnCalendar=` value in UTC: `YYYY-MM-DD HH:MM:SS UTC`.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn systemd_calendar(timestamp: Timestamp) -> String {
    timestamp
        .to_zoned(TimeZone::UTC)
        .strftime("%Y-%m-%d %H:%M:%S UTC")
        .to_string()
}

/// systemd transient unit name for a backend id (`ccplan-<id>`).
#[cfg(any(target_os = "linux", test))]
pub(crate) fn systemd_unit_name(backend_id: &str) -> String {
    format!("ccplan-{backend_id}")
}

/// Extracts `ccplan-<id>.timer` backend ids from `systemctl --user list-timers` output.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn parse_timer_units(output: &str) -> Vec<String> {
    let mut timers = Vec::new();
    for line in output.lines() {
        for field in line.split_whitespace() {
            if let Some(id) = field
                .strip_prefix("ccplan-")
                .and_then(|value| value.strip_suffix(".timer"))
            {
                timers.push(id.to_owned());
            }
        }
    }
    timers.sort();
    timers.dedup();
    timers
}

/// Windows Task Scheduler local boundary `YYYY-MM-DDTHH:MM:SS` rendered in `zone`.
#[cfg(any(target_os = "windows", test))]
pub(crate) fn windows_boundary(timestamp: Timestamp, zone: &TimeZone) -> String {
    timestamp
        .to_zoned(zone.clone())
        .strftime("%Y-%m-%dT%H:%M:%S")
        .to_string()
}

/// Windows Task Scheduler task path for a backend id (`\ccplan\<id>`).
#[cfg(any(target_os = "windows", test))]
pub(crate) fn windows_task_name(backend_id: &str) -> String {
    format!("\\ccplan\\{backend_id}")
}

/// Extracts `\ccplan\<id>` task ids from `schtasks /Query /FO LIST` output.
#[cfg(any(target_os = "windows", test))]
pub(crate) fn parse_task_names(output: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed
            .strip_prefix("TaskName:")
            .map(str::trim)
            .and_then(|value| value.strip_prefix("\\ccplan\\"))
        {
            names.push(name.to_owned());
        }
    }
    names.sort();
    names
}

/// Quotes a single Windows command-line argument (double-quote wrapping + `"` escaping).
#[cfg(any(target_os = "windows", test))]
pub(crate) fn quote_windows_arg(arg: &str) -> String {
    if arg.is_empty()
        || arg
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte == b'"')
    {
        let escaped = arg.replace('"', r#"\""#);
        format!(r#""{escaped}""#)
    } else {
        arg.to_owned()
    }
}

/// XML 1.0 entity escaping, shared by the schtasks XML, the launchd plist, and Windows toasts.
#[cfg(any(target_os = "windows", target_os = "macos", test))]
pub(crate) fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// launchd `LaunchAgent` label for a backend id (`io.ccplan.<id>`).
#[cfg(any(target_os = "macos", test))]
pub(crate) fn launchd_label(backend_id: &str) -> String {
    format!("io.ccplan.{backend_id}")
}

/// launchd `StartCalendarInterval` components for `timestamp` rendered in `zone`.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn calendar_interval(timestamp: Timestamp, zone: &TimeZone) -> CalendarInterval {
    let local = timestamp.to_zoned(zone.clone());
    CalendarInterval {
        month: local.month(),
        day: local.day(),
        hour: local.hour(),
        minute: local.minute(),
        second: local.second(),
    }
}

/// launchd `StartCalendarInterval` fields (minute-granular; `second` is recorded but not honored).
#[cfg(any(target_os = "macos", test))]
pub(crate) struct CalendarInterval {
    pub month: i8,
    pub day: i8,
    pub hour: i8,
    pub minute: i8,
    pub second: i8,
}

/// Quotes a string as an `AppleScript` string literal (used by the macOS notifier).
#[cfg(any(target_os = "macos", test))]
pub(crate) fn applescript_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Quotes a string as a VBScript string literal (used by the Windows notifier).
#[cfg(any(target_os = "windows", test))]
pub(crate) fn vbscript_string(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(instant: &str) -> Timestamp {
        instant.parse::<Timestamp>().unwrap()
    }

    #[test]
    fn systemd_calendar_renders_utc() {
        assert_eq!(
            systemd_calendar(at("2026-06-08T05:30:00Z")),
            "2026-06-08 05:30:00 UTC"
        );
        assert_eq!(systemd_unit_name("abc-0-start"), "ccplan-abc-0-start");
    }

    #[test]
    fn parse_timer_units_extracts_sorts_and_dedups() {
        let output = "\
NEXT LEFT LAST PASSED UNIT ACTIVATES
Sun n/a n/a n/a ccplan-b.timer ccplan-b.service
Mon n/a n/a n/a ccplan-a.timer ccplan-a.service
Tue n/a n/a n/a ccplan-a.timer ccplan-a.service
Wed n/a n/a n/a other.timer other.service
";
        assert_eq!(
            parse_timer_units(output),
            vec!["a".to_owned(), "b".to_owned()]
        );
        assert!(parse_timer_units("no units here").is_empty());
    }

    #[test]
    fn windows_boundary_and_task_name() {
        assert_eq!(
            windows_boundary(at("2026-06-08T05:30:00Z"), &TimeZone::UTC),
            "2026-06-08T05:30:00"
        );
        assert_eq!(windows_task_name("focus-0"), "\\ccplan\\focus-0");
    }

    #[test]
    fn parse_task_names_extracts_and_sorts() {
        let output = "\
Folder: \\ccplan
TaskName: \\ccplan\\zeta
TaskName: \\ccplan\\alpha
TaskName: \\other\\ignored
HostName: PC
";
        assert_eq!(
            parse_task_names(output),
            vec!["alpha".to_owned(), "zeta".to_owned()]
        );
    }

    #[test]
    fn quote_windows_arg_covers_plain_space_quote_and_empty() {
        assert_eq!(quote_windows_arg("plain"), "plain");
        assert_eq!(quote_windows_arg("has space"), "\"has space\"");
        assert_eq!(quote_windows_arg("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote_windows_arg(""), "\"\"");
    }

    #[test]
    fn xml_escape_escapes_all_five_entities() {
        assert_eq!(
            xml_escape("a&b<c>d\"e'f"),
            "a&amp;b&lt;c&gt;d&quot;e&apos;f"
        );
    }

    #[test]
    fn launchd_label_and_calendar_interval() {
        assert_eq!(launchd_label("focus-0"), "io.ccplan.focus-0");
        let interval = calendar_interval(at("2026-06-08T05:30:45Z"), &TimeZone::UTC);
        assert_eq!(
            (
                interval.month,
                interval.day,
                interval.hour,
                interval.minute,
                interval.second
            ),
            (6, 8, 5, 30, 45)
        );
    }

    #[test]
    fn notifier_string_quoters_escape_their_metacharacters() {
        assert_eq!(applescript_string("a\\b\"c"), "\"a\\\\b\\\"c\"");
        assert_eq!(vbscript_string("say \"hi\""), "\"say \"\"hi\"\"\"");
    }
}
