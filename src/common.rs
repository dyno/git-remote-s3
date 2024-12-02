use std::process::Command;
use tracing::debug;

#[allow(dead_code)]
pub fn log_command(cmd: &Command) {
    let program = cmd.get_program().to_str().unwrap_or("[unknown]");
    let args: Vec<_> = cmd
        .get_args()
        .map(|os_str| os_str.to_str().unwrap_or("[invalid]"))
        .collect();
    debug!("Executing command: {} {}", program, args.join(" "));
}
