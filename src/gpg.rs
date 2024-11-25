use super::errors::*;
use std::path::Path;
use std::process::Command;
use std::io::{Write};
use std::fs;
use std::env;

pub fn encrypt(recipients: &[String], i: &Path, o: &Path) -> Result<()> {
    if env::var("GIT_S3_ENCRYPT").unwrap_or_default() != "1" {
        // Just copy the file when encryption is disabled
        fs::copy(i, o).chain_err(|| "failed to copy file")?;
        return Ok(());
    }
    
    let mut cmd = Command::new("gpg");
    cmd.arg("-q").arg("--batch");
    for recipient in recipients {
        cmd.arg("-r").arg(recipient);
    }
    cmd
        .arg("-o")
        .arg(o.to_str().chain_err(|| "out path invalid")?)
        .arg("-e")
        .arg(i.to_str().chain_err(|| "in path invalid")?);
    let result = cmd
        .output()
        .chain_err(|| "failed to run gpg encrypt")?;
    if !result.status.success() {
        write!(std::io::stdout(), "Failed command: {:?}", cmd).unwrap();
        std::io::stdout().write_all(&result.stdout).unwrap();
        std::io::stderr().write_all(&result.stderr).unwrap();
        bail!("gpg encrypt failed");
    }
    Ok(())
}

pub fn decrypt(i: &Path, o: &Path) -> Result<()> {
    if env::var("GIT_S3_ENCRYPT").unwrap_or_default() != "1" {
        // Just copy the file when encryption is disabled
        fs::copy(i, o).chain_err(|| "failed to copy file")?;
        return Ok(());
    }
    
    let mut cmd = Command::new("gpg");
    cmd
        .arg("-q")
        .arg("--batch")
        .arg("-o")
        .arg(o.to_str().chain_err(|| "out path invalid")?)
        .arg("-d")
        .arg(i.to_str().chain_err(|| "in path invalid")?);
    let result = cmd
        .output()
        .chain_err(|| "failed to run gpg decrypt")?;
    if !result.status.success() {
        write!(std::io::stdout(), "Failed command: {:?}", cmd).unwrap();
        std::io::stdout().write_all(&result.stdout).unwrap();
        std::io::stderr().write_all(&result.stderr).unwrap();
        bail!("gpg decrypt failed");
    }
    Ok(())
}
