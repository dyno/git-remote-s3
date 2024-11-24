use super::errors::*;
use std::path::Path;
use std::process::Command;

pub fn bundle_create(bundle: &Path, ref_name: &str) -> Result<()> {
    let result = Command::new("git")
        .arg("bundle")
        .arg("create")
        .arg(bundle.to_str().ok_or_else(|| ErrorKind::GitError("bundle path invalid".to_string()))?)
        .arg(ref_name)
        .output()
        .map_err(|e| ErrorKind::GitError(format!("failed to run git: {}", e)))?;
    if !result.status.success() {
        bail!(ErrorKind::GitError("git bundle failed".to_string()));
    }
    Ok(())
}

pub fn bundle_unbundle(bundle: &Path, ref_name: &str) -> Result<()> {
    let result = Command::new("git")
        .arg("bundle")
        .arg("unbundle")
        .arg(bundle.to_str().ok_or_else(|| ErrorKind::GitError("bundle path invalid".to_string()))?)
        .arg(ref_name)
        .output()
        .map_err(|e| ErrorKind::GitError(format!("failed to run git: {}", e)))?;
    if !result.status.success() {
        bail!(ErrorKind::GitError("git bundle unbundle failed".to_string()));
    }
    Ok(())
}

pub fn is_ancestor(base_ref: &str, remote_ref: &str) -> Result<bool> {
    let result = Command::new("git")
        .arg("merge-base")
        .arg("--is-ancestor")
        .arg(base_ref)
        .arg(remote_ref)
        .output()
        .map_err(|e| ErrorKind::GitError(format!("failed to run git: {}", e)))?;
    Ok(result.status.success())
}

pub fn config(setting: &str) -> Result<String> {
    let result = Command::new("git")
        .arg("config")
        .arg(setting)
        .output()
        .map_err(|e| ErrorKind::GitError(format!("failed to run git: {}", e)))?;
    if !result.status.success() {
        bail!(ErrorKind::GitError(format!("git config {} failed", setting)));
    }
    String::from_utf8(result.stdout)
        .map_err(|e| ErrorKind::GitError(format!("invalid utf8: {}", e)).into())
        .map(|s| s.trim().to_string())
}

pub fn rev_parse(rev: &str) -> Result<String> {
    let result = Command::new("git")
        .arg("rev-parse")
        .arg(rev)
        .output()
        .map_err(|e| ErrorKind::GitError(format!("failed to run git: {}", e)))?;
    if !result.status.success() {
        bail!(ErrorKind::GitError(format!("git rev-parse {} failed", rev)));
    }
    String::from_utf8(result.stdout)
        .map_err(|e| ErrorKind::GitError(format!("invalid utf8: {}", e)).into())
        .map(|s| s.trim().to_string())
}
