use anyhow::{anyhow, Result};
use std::fs;
use std::process::Command;
use tempfile::TempDir;
use tracing::{error, info};

mod common;
use common::init_test_logging;

use git_remote_s3::git;

const TEST_EMAIL: &str = "test@example.com";
const TEST_USER: &str = "Test User";

fn init_git_repo() -> Result<TempDir> {
    // Create a new temp directory for the git repo
    let repo_dir = TempDir::new()?;
    info!("Created temp dir: {:?}", repo_dir.path());

    // Initialize git repo
    let output = Command::new("git")
        .current_dir(repo_dir.path())
        .arg("init")
        .output()?;
    if !output.status.success() {
        error!(
            "git init failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(anyhow!("git init failed"));
    }
    info!("Git init successful");

    // Configure git
    let output = Command::new("git")
        .current_dir(repo_dir.path())
        .args(["config", "--local", "user.email", TEST_EMAIL])
        .output()?;
    if !output.status.success() {
        error!(
            "git config failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(anyhow!("git config failed"));
    }

    let output = Command::new("git")
        .current_dir(repo_dir.path())
        .args(["config", "--local", "user.name", TEST_USER])
        .output()?;
    if !output.status.success() {
        error!(
            "git config failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(anyhow!("git config failed"));
    }
    info!("Git config successful");

    Ok(repo_dir)
}

fn create_commit(repo_dir: &TempDir) -> Result<()> {
    let test_file = repo_dir.path().join("test.txt");
    fs::write(&test_file, "test content")?;

    let output = Command::new("git")
        .current_dir(repo_dir.path())
        .args(["add", "test.txt"])
        .output()?;
    if !output.status.success() {
        error!(
            "git add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(anyhow!("git add failed"));
    }

    let output = Command::new("git")
        .current_dir(repo_dir.path())
        .args(["commit", "-m", "test commit"])
        .output()?;
    if !output.status.success() {
        error!(
            "git commit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(anyhow!("git commit failed"));
    }

    Ok(())
}

#[tokio::test]
async fn test_git_rev_parse() -> Result<()> {
    init_test_logging();
    let repo_dir = init_git_repo()?;
    create_commit(&repo_dir)?;

    let output = Command::new("git")
        .current_dir(repo_dir.path())
        .args(["rev-parse", "HEAD"])
        .output()?;
    if !output.status.success() {
        error!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(anyhow!("git rev-parse failed"));
    }
    let head = String::from_utf8(output.stdout)?.trim().to_string();
    assert!(!head.is_empty());
    assert_eq!(head.len(), 40); // SHA-1 hash is 40 characters

    Ok(())
}

#[tokio::test]
async fn test_git_is_ancestor() -> Result<()> {
    init_test_logging();
    let repo_dir = init_git_repo()?;

    // Create first commit
    let first_file = repo_dir.path().join("first.txt");
    fs::write(&first_file, "first")?;
    let output = Command::new("git")
        .current_dir(repo_dir.path())
        .args(["add", "first.txt"])
        .output()?;
    if !output.status.success() {
        error!(
            "git add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(anyhow!("git add failed"));
    }

    let output = Command::new("git")
        .current_dir(repo_dir.path())
        .args(["commit", "-m", "first"])
        .output()?;
    if !output.status.success() {
        error!(
            "git commit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(anyhow!("git commit failed"));
    }

    let output = Command::new("git")
        .current_dir(repo_dir.path())
        .args(["rev-parse", "HEAD"])
        .output()?;
    if !output.status.success() {
        error!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(anyhow!("git rev-parse failed"));
    }
    let first_commit = String::from_utf8(output.stdout)?.trim().to_string();
    info!(commit = %first_commit, "First commit");

    // Create second commit
    let second_file = repo_dir.path().join("second.txt");
    fs::write(&second_file, "second")?;
    let output = Command::new("git")
        .current_dir(repo_dir.path())
        .args(["add", "second.txt"])
        .output()?;
    if !output.status.success() {
        error!(
            "git add failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(anyhow!("git add failed"));
    }

    let output = Command::new("git")
        .current_dir(repo_dir.path())
        .args(["commit", "-m", "second"])
        .output()?;
    if !output.status.success() {
        error!(
            "git commit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(anyhow!("git commit failed"));
    }

    let output = Command::new("git")
        .current_dir(repo_dir.path())
        .args(["rev-parse", "HEAD"])
        .output()?;
    if !output.status.success() {
        error!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(anyhow!("git rev-parse failed"));
    }
    let second_commit = String::from_utf8(output.stdout)?.trim().to_string();
    info!(commit = %second_commit, "Second commit");

    let output = Command::new("git")
        .current_dir(repo_dir.path())
        .args(["log", "--oneline"])
        .output()?;
    if !output.status.success() {
        error!(
            "git log failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(anyhow!("git log failed"));
    }
    info!(log = %String::from_utf8_lossy(&output.stdout), "Git log");

    // Test ancestry using our git module
    std::env::set_current_dir(repo_dir.path())?;
    assert!(git::is_ancestor(&first_commit, &second_commit)?);
    assert!(!git::is_ancestor(&second_commit, &first_commit)?);

    // Test with non-existent commits
    assert!(!git::is_ancestor("non-existent", &second_commit)?);
    assert!(!git::is_ancestor(&first_commit, "non-existent")?);

    Ok(())
}

#[tokio::test]
async fn test_git_bundle() -> Result<()> {
    init_test_logging();
    let source_dir = init_git_repo()?;
    create_commit(&source_dir)?;

    // Create bundle
    let bundle_file = source_dir.path().join("test.bundle");
    let output = Command::new("git")
        .current_dir(source_dir.path())
        .args(["bundle", "create", bundle_file.to_str().unwrap(), "HEAD"])
        .output()?;
    if !output.status.success() {
        error!(
            "git bundle create failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(anyhow!("git bundle create failed"));
    }

    // Verify bundle contents
    let output = Command::new("git")
        .current_dir(source_dir.path())
        .args(["bundle", "verify", bundle_file.to_str().unwrap()])
        .output()?;
    if !output.status.success() {
        error!(
            "git bundle verify failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(anyhow!("git bundle verify failed"));
    }

    // Initialize target repo and unbundle
    let target_dir = init_git_repo()?;
    let output = Command::new("git")
        .current_dir(target_dir.path())
        .args(["bundle", "unbundle", bundle_file.to_str().unwrap()])
        .output()?;
    if !output.status.success() {
        error!(
            "git bundle unbundle failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(anyhow!("git bundle unbundle failed"));
    }

    Ok(())
}
