//! Windows Task Scheduler backend.
//!
//! Every function here is process/file IO, so each carries a fn-level `coverage(off)`; the pure XML
//! and argument formatting lives in `super::format` (coverage-on, unit-tested on every host). There
//! is intentionally no module-scope `coverage(off)`, so the anti-gaming guard can prove no business
//! logic hides here.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use jiff::{SignedDuration, tz::TimeZone};

use crate::{
    context::{Scheduler, SchedulerError},
    platform::{
        DoctorCheck, fire_args,
        format::{
            parse_task_names, quote_windows_arg, windows_boundary, windows_task_name, xml_escape,
        },
    },
    store::TriggerRecord,
};

#[derive(Debug, Clone)]
pub(crate) struct NativeScheduler {
    binary: PathBuf,
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl NativeScheduler {
    pub(crate) fn new() -> Result<Self, SchedulerError> {
        let binary =
            env::current_exe().map_err(|error| SchedulerError::Operation(error.to_string()))?;
        Ok(Self {
            binary: fire_binary(&binary),
        })
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
impl Scheduler for NativeScheduler {
    fn prepare(&self) -> Result<(), SchedulerError> {
        Ok(())
    }

    fn add(&self, trigger: &TriggerRecord) -> Result<(), SchedulerError> {
        let xml = task_xml(&self.binary, trigger)?;
        let path = temp_xml_path(&trigger.backend_id);
        write_xml_utf8_bom(&path, &xml).map_err(io_error("write task XML"))?;
        let output = Command::new("schtasks.exe")
            .args([
                "/Create",
                "/TN",
                &windows_task_name(&trigger.backend_id),
                "/XML",
            ])
            .arg(&path)
            .arg("/F")
            .output()
            .map_err(command_error("schtasks /Create"))?;
        let _ = fs::remove_file(&path);
        ensure_success("schtasks /Create", &output)
    }

    fn remove(&self, backend_id: &str) -> Result<(), SchedulerError> {
        let output = Command::new("schtasks.exe")
            .args(["/Delete", "/TN", &windows_task_name(backend_id), "/F"])
            .output()
            .map_err(command_error("schtasks /Delete"))?;
        if output.status.success() || is_missing_task(&output) {
            Ok(())
        } else {
            Err(failed_output("schtasks /Delete", &output))
        }
    }

    fn list(&self) -> Result<Vec<String>, SchedulerError> {
        let output = Command::new("schtasks.exe")
            .args(["/Query", "/TN", "\\ccplan\\", "/FO", "LIST"])
            .output()
            .map_err(command_error("schtasks /Query"))?;
        ensure_success("schtasks /Query", &output)?;
        Ok(parse_task_names(&String::from_utf8_lossy(&output.stdout)))
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
pub(crate) fn doctor_check() -> DoctorCheck {
    match Command::new("schtasks.exe").arg("/Query").output() {
        Ok(output) if output.status.success() => {
            DoctorCheck::ok("scheduler", "Windows Task Scheduler is available")
        }
        Ok(output) => DoctorCheck::error(
            "scheduler",
            output_summary("schtasks /Query", &output),
            "run from an interactive Windows user session with Task Scheduler enabled",
        ),
        Err(error) => DoctorCheck::error(
            "scheduler",
            error.to_string(),
            "ensure schtasks.exe is available on PATH",
        ),
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn task_xml(binary: &Path, trigger: &TriggerRecord) -> Result<String, SchedulerError> {
    let zone = TimeZone::system();
    let start = windows_boundary(trigger.scheduled_at, &zone);
    let end = trigger
        .scheduled_at
        .checked_add(SignedDuration::from_secs(600))
        .map_err(|error| SchedulerError::Operation(error.to_string()))?;
    let end = windows_boundary(end, &zone);
    let arguments = fire_args(trigger)
        .iter()
        .map(|arg| quote_windows_arg(arg))
        .collect::<Vec<_>>()
        .join(" ");
    let working_dir = binary
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    Ok(format!(
            r#"<?xml version="1.0"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers>
    <TimeTrigger>
      <StartBoundary>{start}</StartBoundary>
      <EndBoundary>{end}</EndBoundary>
      <Enabled>true</Enabled>
    </TimeTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <Enabled>true</Enabled>
    <Hidden>true</Hidden>
    <ExecutionTimeLimit>PT1M</ExecutionTimeLimit>
    <DeleteExpiredTaskAfter>PT0S</DeleteExpiredTaskAfter>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{}</Command>
      <Arguments>{}</Arguments>
      <WorkingDirectory>{}</WorkingDirectory>
    </Exec>
  </Actions>
</Task>"#,
        xml_escape(&binary.display().to_string()),
        xml_escape(&arguments),
        xml_escape(&working_dir)
    ))
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn fire_binary(binary: &Path) -> PathBuf {
    let candidate = binary.with_file_name("ccplan-fire.exe");
    if candidate.exists() {
        candidate
    } else {
        binary.to_path_buf()
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn temp_xml_path(backend_id: &str) -> PathBuf {
    env::temp_dir().join(format!("ccplan-{backend_id}-{}.xml", std::process::id()))
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn command_error(action: &'static str) -> impl FnOnce(std::io::Error) -> SchedulerError {
    move |error| SchedulerError::Operation(format!("{action} failed: {error}"))
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn io_error(action: &'static str) -> impl FnOnce(std::io::Error) -> SchedulerError {
    move |error| SchedulerError::Operation(format!("{action} failed: {error}"))
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn ensure_success(action: &str, output: &Output) -> Result<(), SchedulerError> {
    if output.status.success() {
        Ok(())
    } else {
        Err(failed_output(action, output))
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn failed_output(action: &str, output: &Output) -> SchedulerError {
    SchedulerError::Operation(output_summary(action, output))
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn output_summary(action: &str, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let message = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    format!("{action} exited with {}: {message}", output.status)
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn is_missing_task(output: &Output) -> bool {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    text.contains("cannot find") || text.contains("does not exist")
}

/// 以 UTF-8 带 BOM 编码写入 XML 文件。
/// Windows 任务计划程序对编码很敏感，UTF-8 带 BOM 是最可靠的格式。
#[cfg_attr(coverage_nightly, coverage(off))]
fn write_xml_utf8_bom(path: &Path, xml: &str) -> std::io::Result<()> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    bytes.extend_from_slice(xml.as_bytes());
    fs::write(path, bytes)
}
