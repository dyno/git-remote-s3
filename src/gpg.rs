use std::fs;
use std::path::Path;
use std::process::Command;
use anyhow::{Result, anyhow, bail};

pub fn encrypt(recipients: &[String], i: &Path, o: &Path) -> Result<()> {
    if recipients.is_empty() {
        fs::copy(i, o).map_err(|e| anyhow!("failed to copy file: {}", e))?;
        return Ok(());
    }

    let mut cmd = Command::new("gpg");
    cmd.arg("--batch")
        .arg("--yes")
        .arg("--encrypt");

    for r in recipients {
        cmd.arg("-r").arg(r);
    }

    cmd.arg("--output")
        .arg(o.to_str().ok_or_else(|| anyhow!("out path invalid"))?);
    cmd.arg(i.to_str().ok_or_else(|| anyhow!("in path invalid"))?);

    let result = cmd
        .output()
        .map_err(|e| anyhow!("failed to run gpg encrypt: {}", e))?;

    if !result.status.success() {
        bail!("gpg encrypt failed");
    }

    Ok(())
}

pub fn decrypt(i: &Path, o: &Path) -> Result<()> {
    if !i.exists() {
        fs::copy(i, o).map_err(|e| anyhow!("failed to copy file: {}", e))?;
        return Ok(());
    }

    let mut cmd = Command::new("gpg");
    cmd.arg("--batch")
        .arg("--yes")
        .arg("--decrypt");

    cmd.arg("--output")
        .arg(o.to_str().ok_or_else(|| anyhow!("out path invalid"))?);
    cmd.arg(i.to_str().ok_or_else(|| anyhow!("in path invalid"))?);

    let result = cmd
        .output()
        .map_err(|e| anyhow!("failed to run gpg decrypt: {}", e))?;

    if !result.status.success() {
        bail!("gpg decrypt failed");
    }

    Ok(())
}
