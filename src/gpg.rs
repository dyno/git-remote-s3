use std::fs;
use std::path::Path;
use std::process::Command;
use anyhow::{Result, anyhow, bail};
use tracing::{debug, error};
use crate::common::log_command;

pub fn encrypt(recipients: &[String], i: &Path, o: &Path) -> Result<()> {
    if recipients.is_empty() {
        debug!("No GPG recipients specified, copying file without encryption");
        fs::copy(i, o).map_err(|e| anyhow!("failed to copy file: {}", e))?;
        return Ok(());
    }

    debug!(?recipients, ?i, ?o, "Encrypting file with GPG");
    let mut cmd = Command::new("gpg");
    cmd.arg("--batch")
        .arg("--yes")
        .arg("--encrypt");

    for r in recipients {
        cmd.arg("-r").arg(r);
    }

    cmd.arg("--output")
        .arg(o.to_str().ok_or_else(|| anyhow!("out path invalid"))?)
        .arg(i.to_str().ok_or_else(|| anyhow!("in path invalid"))?);

    log_command(&cmd);

    let result = cmd
        .output()
        .map_err(|e| anyhow!("failed to run gpg encrypt: {}", e))?;

    if !result.status.success() {
        error!("GPG encryption failed");
        bail!("gpg encrypt failed");
    }

    debug!("File encrypted successfully");
    Ok(())
}

pub fn decrypt(i: &Path, o: &Path) -> Result<()> {
    if !i.exists() {
        debug!("Input file doesn't exist, copying file without decryption");
        fs::copy(i, o).map_err(|e| anyhow!("failed to copy file: {}", e))?;
        return Ok(());
    }

    debug!(?i, ?o, "Decrypting file with GPG");
    let mut cmd = Command::new("gpg");
    cmd.arg("--batch")
        .arg("--yes")
        .arg("--decrypt")
        .arg("--output")
        .arg(o.to_str().ok_or_else(|| anyhow!("out path invalid"))?)
        .arg(i.to_str().ok_or_else(|| anyhow!("in path invalid"))?);

    log_command(&cmd);

    let result = cmd
        .output()
        .map_err(|e| anyhow!("failed to run gpg decrypt: {}", e))?;

    if !result.status.success() {
        error!("GPG decryption failed");
        bail!("gpg decrypt failed");
    }

    debug!("File decrypted successfully");
    Ok(())
}
