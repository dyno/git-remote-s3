use crate::common::log_command;
use anyhow::{anyhow, bail, Result};
use std::path::Path;
use std::process::Command;
use tracing::{debug, error};

pub fn bundle_create(bundle: &Path, ref_name: &str, dir: &Path) -> Result<()> {
    debug!(?bundle, ?ref_name, "Creating git bundle");

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

    let result = cmd.output().map_err(|e| {
        error!(?e, "Failed to run git bundle create");
        anyhow!("failed to run git: {}", e)
    })?;

    if !result.status.success() {
        error!(?bundle, ?ref_name, "Git bundle create command failed");
        bail!("git bundle failed");
    }

    debug!("Bundle created successfully");
    Ok(())
}

pub fn bundle_unbundle(bundle: &Path, ref_name: &str, dir: &Path) -> Result<()> {
    debug!(?bundle, ?ref_name, "Unbundling git bundle");

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

    let result = cmd.output().map_err(|e| {
        error!(?e, "Failed to run git bundle unbundle");
        anyhow!("failed to run git: {}", e)
    })?;

    if !result.status.success() {
        error!(?bundle, ?ref_name, "Git bundle unbundle command failed");
        bail!("git bundle failed");
    }

    debug!("Bundle unbundled successfully");
    Ok(())
}

pub fn config(setting: &str, dir: &Path) -> Result<String> {
    debug!(?setting, "Reading git config");

    let mut cmd = Command::new("git");
    cmd.arg("config").arg(setting).current_dir(dir);

    log_command(&cmd);

    let result = cmd.output().map_err(|e| {
        error!(?e, "Failed to run git config");
        anyhow!("failed to run git: {}", e)
    })?;

    if !result.status.success() {
        error!(?setting, "Git config command failed");
        bail!("git config failed");
    }

    let output = String::from_utf8(result.stdout)
        .map_err(|e| anyhow!("git config output not utf8: {}", e))?
        .trim()
        .to_string();

    debug!(?output, "Git config read successfully");
    Ok(output)
}

pub fn is_ancestor(base_ref: &str, remote_ref: &str, dir: &Path) -> Result<bool> {
    debug!(?base_ref, ?remote_ref, "Checking git ancestry");

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
        debug!(?base_ref, ?remote_ref, "One or both refs don't exist");
        return Ok(false);
    }

    let mut cmd = Command::new("git");
    cmd.arg("merge-base")
        .arg("--is-ancestor")
        .arg(base_ref)
        .arg(remote_ref)
        .current_dir(dir);

    log_command(&cmd);

    let result = cmd.output().map_err(|e| {
        error!(?e, "Failed to run git merge-base");
        anyhow!("failed to run git: {}", e)
    })?;

    if !result.status.success() && result.status.code() != Some(1) {
        error!(?base_ref, ?remote_ref, "Git merge-base command failed");
        bail!("git merge-base failed");
    }

    Ok(result.status.success())
}

pub fn rev_parse(rev: &str, dir: &Path) -> Result<String> {
    debug!(?rev, "Resolving git revision");

    let mut cmd = Command::new("git");
    cmd.arg("rev-parse").arg(rev).current_dir(dir);

    log_command(&cmd);

    let result = cmd.output().map_err(|e| {
        error!(?e, "Failed to run git rev-parse");
        anyhow!("failed to run git: {}", e)
    })?;

    if !result.status.success() {
        error!(?rev, "Git rev-parse command failed");
        bail!("git rev-parse failed");
    }

    let output = String::from_utf8(result.stdout)
        .map_err(|e| anyhow!("git rev-parse output not utf8: {}", e))?
        .trim()
        .to_string();

    debug!(?output, "Git revision resolved successfully");
    Ok(output)
}
