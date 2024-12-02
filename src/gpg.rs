use crate::common::log_command;
use anyhow::{anyhow, Result};
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::{debug, error, instrument};

/// Encrypt a file with GPG
#[instrument]
pub fn encrypt(recipients: &[String], input: &Path, output: &Path) -> Result<()> {
    if recipients.is_empty() {
        debug!("No GPG recipients specified, copying file without encryption");
        fs::copy(input, output).map_err(|e| anyhow!("failed to copy file: {}", e))?;
        return Ok(());
    }

    let mut cmd = Command::new("gpg");
    cmd.arg("--batch")
        .arg("--yes")
        .arg("--output")
        .arg(output)
        .arg("--encrypt");

    for r in recipients {
        cmd.arg("--recipient").arg(r);
    }

    cmd.arg(input);

    log_command(&cmd);

    let output = cmd.output()?;
    if !output.status.success() {
        error!(?input, ?recipients, stderr=?String::from_utf8_lossy(&output.stderr), "GPG encryption failed");
        return Err(anyhow!("gpg encrypt failed"));
    }

    Ok(())
}

/// Decrypt a file with GPG
#[instrument]
pub fn decrypt(input: &Path, output: &Path) -> Result<()> {
    if !input.exists() {
        debug!("Input file doesn't exist, copying file without decryption");
        fs::copy(input, output).map_err(|e| anyhow!("failed to copy file: {}", e))?;
        return Ok(());
    }

    let mut cmd = Command::new("gpg");
    cmd.arg("--batch")
        .arg("--yes")
        .arg("--output")
        .arg(output)
        .arg("--decrypt")
        .arg(input);

    log_command(&cmd);

    let output = cmd.output()?;
    if !output.status.success() {
        error!(?input, stderr=?String::from_utf8_lossy(&output.stderr), "GPG decryption failed");
        return Err(anyhow!("gpg decrypt failed"));
    }

    Ok(())
}
