#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use std::process::ExitCode;

use clap::Parser;

// Windows Task Scheduler uses this GUI-subsystem wrapper for fire invocations
// so no console window flashes. Child wscript processes are spawned with
// CREATE_NO_WINDOW; MsgBox UI still appears in the interactive session.
#[cfg_attr(coverage_nightly, coverage(off))]
fn main() -> ExitCode {
    let cli = ccplan::cli::Cli::parse();

    match ccplan::run(cli, std::io::stdout()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(error.exit_code())
        }
    }
}
