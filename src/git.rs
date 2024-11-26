use std::process::Command;
use anyhow::{Result, anyhow, bail};
use std::path::Path;
use tracing::{debug, error, instrument};

#[instrument]
pub fn bundle_create(bundle: &Path, ref_name: &str) -> Result<()> {
    debug!(?bundle, ?ref_name, "Creating git bundle");
    
    let result = Command::new("git")
        .arg("bundle")
        .arg("create")
        .arg(bundle.to_str().ok_or_else(|| {
            error!(?bundle, "Bundle path is not valid UTF-8");
            anyhow!("bundle path invalid")
        })?)
        .arg(ref_name)
        .output()
        .map_err(|e| {
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

#[instrument]
pub fn bundle_unbundle(bundle: &Path, ref_name: &str) -> Result<()> {
    debug!(?bundle, ?ref_name, "Unbundling git bundle");
    
    let result = Command::new("git")
        .arg("bundle")
        .arg("unbundle")
        .arg(bundle.to_str().ok_or_else(|| {
            error!(?bundle, "Bundle path is not valid UTF-8");
            anyhow!("bundle path invalid")
        })?)
        .arg(ref_name)
        .output()
        .map_err(|e| {
            error!(?e, "Failed to run git bundle unbundle");
            anyhow!("failed to run git: {}", e)
        })?;

    if !result.status.success() {
        error!(?bundle, ?ref_name, "Git bundle unbundle command failed");
        bail!("git unbundle failed");
    }
    
    debug!("Bundle unbundled successfully");
    Ok(())
}

#[instrument]
pub fn config(setting: &str) -> Result<String> {
    debug!(?setting, "Reading git config");
    
    let result = Command::new("git")
        .arg("config")
        .arg(setting)
        .output()
        .map_err(|e| {
            error!(?e, "Failed to run git config");
            anyhow!("failed to run git: {}", e)
        })?;

    if !result.status.success() {
        error!(?setting, "Git config command failed");
        bail!("git config failed");
    }

    String::from_utf8(result.stdout)
        .map_err(|e| {
            error!(?e, "Config output is not UTF-8");
            anyhow!("not utf8: {}", e)
        })
        .map(|s| s.trim().to_string())
}

#[instrument]
pub fn is_ancestor(base_ref: &str, remote_ref: &str) -> Result<bool> {
    debug!(?base_ref, ?remote_ref, "Checking git ancestry");
    
    let result = Command::new("git")
        .arg("merge-base")
        .arg("--is-ancestor")
        .arg(remote_ref)
        .arg(base_ref)
        .output()
        .map_err(|e| {
            error!(?e, "Failed to run git merge-base");
            anyhow!("failed to run git: {}", e)
        })?;

    debug!(?base_ref, ?remote_ref, success = ?result.status.success(), "Ancestry check complete");
    Ok(result.status.success())
}

#[instrument]
pub fn rev_parse(rev: &str) -> Result<String> {
    debug!(?rev, "Resolving git revision");
    
    let result = Command::new("git")
        .arg("rev-parse")
        .arg(rev)
        .output()
        .map_err(|e| {
            error!(?e, "Failed to run git rev-parse");
            anyhow!("failed to run git: {}", e)
        })?;

    if !result.status.success() {
        error!(?rev, "Git rev-parse command failed");
        bail!("git rev-parse failed");
    }

    String::from_utf8(result.stdout)
        .map_err(|e| {
            error!(?e, "Rev-parse output is not UTF-8");
            anyhow!("not utf8: {}", e)
        })
        .map(|s| s.trim().to_string())
}
