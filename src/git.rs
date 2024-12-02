use crate::common::log_command;
use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;
use tracing::{error, instrument};

#[instrument]
pub fn bundle_create(bundle: &Path, ref_name: &str, dir: &Path) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("bundle")
        .arg("create")
        .arg(bundle.to_str().ok_or_else(|| {
            error!(?bundle, "Bundle path is not valid UTF-8");
            anyhow!("bundle path invalid")
        })?)
        .arg(ref_name)
        .current_dir(dir);

    log_command(&cmd);

    let output = cmd.output().map_err(|e| {
        error!(?e, "Failed to run git bundle create");
        anyhow!("failed to run git: {}", e)
    })?;

    if !output.status.success() {
        error!(?bundle, ?ref_name, "Git bundle create command failed");
        return Err(anyhow!("git bundle failed"));
    }

    Ok(())
}

#[instrument]
pub fn bundle_unbundle(bundle: &Path, ref_name: &str, dir: &Path) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("bundle")
        .arg("unbundle")
        .arg(bundle.to_str().ok_or_else(|| {
            error!(?bundle, "Bundle path is not valid UTF-8");
            anyhow!("bundle path invalid")
        })?)
        .current_dir(dir);

    if !ref_name.is_empty() {
        cmd.arg(ref_name);
    }

    log_command(&cmd);

    let output = cmd.output().map_err(|e| {
        error!(?e, "Failed to run git bundle unbundle");
        anyhow!("failed to run git: {}", e)
    })?;

    if !output.status.success() {
        error!(?bundle, ?ref_name, "Git bundle unbundle command failed");
        return Err(anyhow!("git bundle failed"));
    }

    Ok(())
}

#[instrument]
pub fn config(setting: &str, dir: &Path) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.arg("config").arg(setting).current_dir(dir);

    log_command(&cmd);

    let output = cmd.output().map_err(|e| {
        error!(?e, "Failed to run git config");
        anyhow!("failed to run git: {}", e)
    })?;

    if !output.status.success() {
        error!(?setting, "Git config command failed");
        return Err(anyhow!("git config failed"));
    }

    String::from_utf8(output.stdout)
        .map_err(|e| anyhow!("git config output not utf8: {}", e))
        .map(|s| s.trim().to_string())
}

#[instrument]
pub fn is_ancestor(base_ref: &str, remote_ref: &str, dir: &Path) -> Result<bool> {
    // First check if both refs exist
    let mut cmd = Command::new("git");
    cmd.arg("rev-parse")
        .arg("--quiet")
        .arg("--verify")
        .arg(format!("{}^{{commit}}", base_ref))
        .current_dir(dir);

    log_command(&cmd);

    let base_exists = cmd
        .output()
        .map_err(|e| {
            error!(?e, "Failed to run git rev-parse");
            anyhow!("failed to run git: {}", e)
        })?
        .status
        .success();

    let mut cmd = Command::new("git");
    cmd.arg("rev-parse")
        .arg("--quiet")
        .arg("--verify")
        .arg(format!("{}^{{commit}}", remote_ref))
        .current_dir(dir);

    log_command(&cmd);

    let remote_exists = cmd
        .output()
        .map_err(|e| {
            error!(?e, "Failed to run git rev-parse");
            anyhow!("failed to run git: {}", e)
        })?
        .status
        .success();

    if !base_exists || !remote_exists {
        return Ok(false);
    }

    let mut cmd = Command::new("git");
    cmd.arg("merge-base")
        .arg("--is-ancestor")
        .arg(base_ref)
        .arg(remote_ref)
        .current_dir(dir);

    log_command(&cmd);

    let output = cmd.output().map_err(|e| {
        error!(?e, "Failed to run git merge-base");
        anyhow!("failed to run git: {}", e)
    })?;

    if !output.status.success() && output.status.code() != Some(1) {
        error!(?base_ref, ?remote_ref, "Git merge-base command failed");
        return Err(anyhow!("git merge-base failed"));
    }

    Ok(output.status.success())
}

#[instrument]
pub fn rev_parse(rev: &str, dir: &Path) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.arg("rev-parse").arg(rev).current_dir(dir);

    log_command(&cmd);

    let output = cmd.output().map_err(|e| {
        error!(?e, "Failed to run git rev-parse");
        anyhow!("failed to run git: {}", e)
    })?;

    if !output.status.success() {
        error!(?rev, "Git rev-parse command failed");
        return Err(anyhow!("git rev-parse failed"));
    }

    String::from_utf8(output.stdout)
        .map_err(|e| anyhow!("git rev-parse output not utf8: {}", e))
        .map(|s| s.trim().to_string())
}
