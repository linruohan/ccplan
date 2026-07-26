//! Desktop notification backend.
//!
//! Every function here is environment/process/D-Bus IO, so each carries a fn-level `coverage(off)`
//! (the real notifier is never driven by the in-process test suite, which uses a recording fake).
//! The pure string quoters/escapers it needs on macOS/Windows live in `super::format` (coverage-on,
//! unit-tested on every host). There is intentionally no module-scope `coverage(off)`.

use crate::{
    context::{Notification, Notifier, NotifyError},
    platform::DoctorCheck,
};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NativeNotifier;

#[cfg_attr(coverage_nightly, coverage(off))]
impl Notifier for NativeNotifier {
    fn check(&self) -> Result<(), NotifyError> {
        check_notification_environment()
    }

    fn notify(&self, notification: &Notification) -> Result<(), NotifyError> {
        check_notification_environment()?;
        send_native_notification(notification)
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
pub(crate) fn doctor_check() -> DoctorCheck {
    match check_notification_environment() {
        Ok(()) => DoctorCheck::ok("notifier", "desktop notification environment is present"),
        Err(error) => DoctorCheck::warning(
            "notifier",
            error.to_string(),
            "run `ccplan doctor` inside a graphical desktop session, then rerun `ccplan apply`",
        ),
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn check_notification_environment() -> Result<(), NotifyError> {
    platform_notification_check()
}

#[cfg(all(unix, not(target_os = "macos")))]
#[cfg_attr(coverage_nightly, coverage(off))]
fn platform_notification_check() -> Result<(), NotifyError> {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some() {
        return Ok(());
    }
    if runtime_bus_path()
        .as_deref()
        .is_some_and(|path| std::path::Path::new(path).exists())
    {
        return Ok(());
    }
    Err(NotifyError::Operation(
        "DBUS_SESSION_BUS_ADDRESS is missing and /run/user/<uid>/bus is unavailable".to_owned(),
    ))
}

#[cfg(target_os = "macos")]
#[cfg_attr(coverage_nightly, coverage(off))]
fn platform_notification_check() -> Result<(), NotifyError> {
    command_available("osascript", "osascript is unavailable")
}

#[cfg(target_os = "windows")]
#[cfg_attr(coverage_nightly, coverage(off))]
fn platform_notification_check() -> Result<(), NotifyError> {
    let system_root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let path = std::path::Path::new(&system_root)
        .join("System32")
        .join("wscript.exe");
    if path.is_file() {
        Ok(())
    } else {
        Err(NotifyError::Operation(format!(
            "wscript.exe is unavailable at {}",
            path.display()
        )))
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
#[cfg_attr(coverage_nightly, coverage(off))]
fn platform_notification_check() -> Result<(), NotifyError> {
    Err(NotifyError::Unavailable)
}

#[cfg(target_os = "macos")]
#[cfg_attr(coverage_nightly, coverage(off))]
fn command_available(command: &str, message: &str) -> Result<(), NotifyError> {
    std::process::Command::new(command)
        .arg("--help")
        .output()
        .map(|_| ())
        .map_err(|error| NotifyError::Operation(format!("{message}: {error}")))
}

#[cfg(all(unix, not(target_os = "macos")))]
#[cfg_attr(coverage_nightly, coverage(off))]
fn runtime_bus_path() -> Option<String> {
    std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|value| !value.is_empty())
        .map(|dir| format!("{dir}/bus"))
}

#[cfg(all(unix, not(target_os = "macos")))]
#[cfg_attr(coverage_nightly, coverage(off))]
fn send_native_notification(notification: &Notification) -> Result<(), NotifyError> {
    notify_rust::Notification::new()
        .summary(&notification.title)
        .body(&notification.body)
        .show()
        .map(|_| ())
        .map_err(|error| NotifyError::Operation(error.to_string()))
}

#[cfg(target_os = "macos")]
#[cfg_attr(coverage_nightly, coverage(off))]
fn send_native_notification(notification: &Notification) -> Result<(), NotifyError> {
    let script = format!(
        "display notification {} with title {}",
        super::format::applescript_string(&notification.body),
        super::format::applescript_string(&notification.title)
    );
    let output = std::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .map_err(|error| NotifyError::Operation(format!("osascript failed: {error}")))?;
    command_success("osascript", &output)
}

/// Windows notifications use `wscript` + `MsgBox` (same idea as a desktop
/// `.vbs` reminder created with `schtasks /Create /TR ...`).
///
/// Scripts are persisted under `%LOCALAPPDATA%\ccplan\data` (or
/// `$CCPLAN_ROOT\data` when set) so they can be inspected after a fire.
#[cfg(target_os = "windows")]
#[cfg_attr(coverage_nightly, coverage(off))]
fn send_native_notification(notification: &Notification) -> Result<(), NotifyError> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // vbInformation
    const MSGBOX_INFORMATION: i32 = 64;

    // MsgBox prompt is the prominent text; Notification.title maps there (not the
    // window caption). Append the body with `vbCrLf` — VBScript string literals
    // cannot contain raw newlines (that yields "unterminated string constant").
    let prompt = if notification.body.is_empty() {
        super::format::vbscript_string(&notification.title)
    } else {
        format!(
            "{} & vbCrLf & {}",
            super::format::vbscript_string(&notification.title),
            super::format::vbscript_string(&notification.body)
        )
    };
    let script = format!(
        "MsgBox {prompt}, {style}, {caption}\r\n",
        prompt = prompt,
        style = MSGBOX_INFORMATION,
        caption = super::format::vbscript_string("ccplan"),
    );
    let dir = notify_scripts_dir()?;
    let path = dir.join(format!(
        "notify-{}-{}.vbs",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    // UTF-16 LE with BOM: wscript handles Unicode VBScript files reliably.
    let mut bytes = vec![0xFF, 0xFE];
    for unit in script.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    std::fs::write(&path, bytes)
        .map_err(|error| NotifyError::Operation(format!("write notify script failed: {error}")))?;

    let output = std::process::Command::new("wscript.exe")
        .args([
            "//Nologo",
            path.to_str().ok_or_else(|| {
                NotifyError::Operation("notify script path is not valid UTF-8".to_owned())
            })?,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| NotifyError::Operation(format!("wscript notify failed: {error}")))?;
    command_success("wscript notify", &output)
}

/// `%LOCALAPPDATA%\ccplan\data`, or `$CCPLAN_ROOT\data` under tests/custom roots.
#[cfg(target_os = "windows")]
#[cfg_attr(coverage_nightly, coverage(off))]
fn notify_scripts_dir() -> Result<std::path::PathBuf, NotifyError> {
    let dir = if let Some(root) = std::env::var_os("CCPLAN_ROOT") {
        std::path::PathBuf::from(root).join("data")
    } else {
        let local = std::env::var_os("LOCALAPPDATA").ok_or_else(|| {
            NotifyError::Operation("LOCALAPPDATA is unset".to_owned())
        })?;
        std::path::PathBuf::from(local).join("ccplan").join("data")
    };
    std::fs::create_dir_all(&dir).map_err(|error| {
        NotifyError::Operation(format!(
            "create notify script dir {} failed: {error}",
            dir.display()
        ))
    })?;
    Ok(dir)
}

#[cfg(not(any(unix, target_os = "windows")))]
#[cfg_attr(coverage_nightly, coverage(off))]
fn send_native_notification(_notification: &Notification) -> Result<(), NotifyError> {
    Err(NotifyError::Unavailable)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[cfg_attr(coverage_nightly, coverage(off))]
fn command_success(command: &str, output: &std::process::Output) -> Result<(), NotifyError> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let message = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    Err(NotifyError::Operation(format!(
        "{command} exited with {}: {message}",
        output.status
    )))
}
