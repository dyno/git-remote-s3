use super::errors::*;
use std::path::Path;
use std::process::Command;
use std::io::Write;

pub fn encrypt(recipients: &[String], i: &Path, o: &Path) -> Result<()> {
    let mut cmd = Command::new("gpg");
    cmd.arg("-q").arg("--batch");
    for recipient in recipients {
        cmd.arg("-r").arg(recipient);
    }
    cmd
        .arg("-o")
        .arg(o.to_str().ok_or_else(|| ErrorKind::GpgError("out path invalid".to_string()))?)
        .arg("-e")
        .arg(i.to_str().ok_or_else(|| ErrorKind::GpgError("in path invalid".to_string()))?);
    let result = cmd
        .output()
        .map_err(|e| ErrorKind::GpgError(format!("failed to run gpg encrypt: {}", e)))?;
    if !result.status.success() {
        write!(std::io::stdout(), "Failed command: {:?}", cmd).unwrap();
        bail!(ErrorKind::GpgError("gpg encrypt failed".to_string()));
    }
    Ok(())
}

pub fn decrypt(i: &Path, o: &Path) -> Result<()> {
    let mut cmd = Command::new("gpg");
    cmd.arg("-q")
        .arg("--batch")
        .arg("-o")
        .arg(o.to_str().ok_or_else(|| ErrorKind::GpgError("out path invalid".to_string()))?)
        .arg("-d")
        .arg(i.to_str().ok_or_else(|| ErrorKind::GpgError("in path invalid".to_string()))?);
    let result = cmd
        .output()
        .map_err(|e| ErrorKind::GpgError(format!("failed to run gpg decrypt: {}", e)))?;
    if !result.status.success() {
        write!(std::io::stdout(), "Failed command: {:?}", cmd).unwrap();
        bail!(ErrorKind::GpgError("gpg decrypt failed".to_string()));
    }
    Ok(())
}
