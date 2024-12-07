use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;
use tracing::{error, instrument};

#[instrument]
pub fn bundle_create(bundle: &Path, ref_name: &str, current_dir: &Path) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("bundle")
        .arg("create")
        .arg(bundle.to_str().ok_or_else(|| {
            error!(?bundle, "Bundle path is not valid UTF-8");
            anyhow!("bundle path invalid")
        })?)
        .arg(ref_name);

    let output = cmd.current_dir(&current_dir).output()?;
    if !output.status.success() {
        error!(?bundle, ?ref_name, "Git bundle create command failed");
        return Err(anyhow!("git bundle create failed"));
    }

    Ok(())
}

#[instrument]
pub fn bundle_unbundle(bundle: &Path, ref_name: &str, current_dir: &Path) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("bundle").arg("unbundle").arg(bundle);

    if !ref_name.is_empty() {
        cmd.arg(ref_name);
    }

    let output = cmd.current_dir(&current_dir).output()?;
    if !output.status.success() {
        let stderr = String::from_utf8(output.stderr.clone()).unwrap_or_default();
        error!(
            ?bundle,
            ?ref_name,
            ?stderr,
            "Git bundle unbundle command failed"
        );
        return Err(anyhow!("git bundle unbundle failed"));
    }

    Ok(())
}

#[instrument]
pub fn is_ancestor(base_ref: &str, remote_ref: &str, current_dir: &Path) -> Result<bool> {
    let mut cmd = Command::new("git");
    cmd.args(["merge-base", "--is-ancestor", base_ref, remote_ref]);

    cmd.current_dir(&current_dir)
        .output()
        .map(|output| output.status.success() && output.status.code() == Some(0))
        .map_err(|err| {
            error!(?cmd, ?err, "command execute failed");
            anyhow!("command execute failed: {:?}", err)
        })
        .or_else(|_| Ok(false))
}

#[instrument]
pub fn rev_parse(rev: &str, current_dir: &Path) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.arg("rev-parse").arg(rev);

    let output = cmd.current_dir(&current_dir).output()?;
    if !output.status.success() {
        error!(?rev, "Git rev-parse command failed");
        return Err(anyhow!("git rev-parse failed"));
    }

    String::from_utf8(output.stdout)
        .map_err(|e| anyhow!("git rev-parse output not utf8: {}", e))
        .map(|s| s.trim().to_string())
}

// Read a git config setting
#[instrument]
pub fn config(setting: &str, current_dir: &Path) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(["config", setting]);

    cmd.current_dir(&current_dir).output().map(|output| {
        if !output.status.success() {
            error!(?cmd, ?output.stderr, "Command failed");
            return Err(anyhow!("git config failed"));
        }
        String::from_utf8(output.stdout)
            .map_err(|e| anyhow!("git config output not utf8: {}", e))
            .map(|s| s.trim().to_string())
    })?
}
