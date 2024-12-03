use anyhow::Result;
use std::fs;
use std::process::Command;
use tempfile::TempDir;
use tracing::info;

mod common;
use common::init_test_logging;

use git_remote_s3::git;

fn init_git_repo(dir: &TempDir) -> Result<()> {
    Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()?;

    // Set git user config
    Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "config",
            "--local",
            "user.name",
            "Test",
        ])
        .current_dir(dir.path())
        .output()?;
    Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "config",
            "--local",
            "user.email",
            "test@example.com",
        ])
        .current_dir(dir.path())
        .output()?;

    Ok(())
}

fn create_commit(dir: &TempDir) -> Result<()> {
    let test_file = dir.path().join("test.txt");
    fs::write(&test_file, "test content")?;
    Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(dir.path())
        .output()?;
    Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "test commit",
        ])
        .current_dir(dir.path())
        .output()?;
    Ok(())
}

#[test]
fn test_git_rev_parse() -> Result<()> {
    init_test_logging();
    let dir = TempDir::new()?;
    init_git_repo(&dir)?;
    create_commit(&dir)?;

    let head = git::rev_parse("HEAD", dir.path())?;
    assert!(!head.is_empty());
    assert_eq!(head.len(), 40); // SHA-1 hash is 40 characters

    Ok(())
}

#[test]
fn test_git_config() -> Result<()> {
    init_test_logging();
    let dir = TempDir::new()?;
    init_git_repo(&dir)?;

    Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "config",
            "--local",
            "test.key",
            "test value",
        ])
        .current_dir(dir.path())
        .output()?;

    let value = git::config("test.key", dir.path())?;
    assert_eq!(value, "test value");

    Ok(())
}

#[test]
fn test_git_is_ancestor() -> Result<()> {
    init_test_logging();
    let dir = TempDir::new()?;
    init_git_repo(&dir)?;

    // Create first commit
    create_commit(&dir)?;
    let first_commit = git::rev_parse("HEAD", dir.path())?;
    info!(commit = %first_commit, "First commit");

    // Create second commit
    let test_file = dir.path().join("test2.txt");
    fs::write(&test_file, "more content")?;
    Command::new("git")
        .args(["add", "test2.txt"])
        .current_dir(dir.path())
        .output()?;
    Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "second commit",
        ])
        .current_dir(dir.path())
        .output()?;

    // Get second commit hash
    let second_commit = git::rev_parse("HEAD", dir.path())?;
    info!(commit = %second_commit, "Second commit");

    // Print git log
    let log = Command::new("git")
        .args(["log", "--oneline"])
        .current_dir(dir.path())
        .output()?;
    info!(log = %String::from_utf8_lossy(&log.stdout), "Git log");

    // Test ancestry
    let result = git::is_ancestor(&first_commit, &second_commit, dir.path())?;
    info!(first = %first_commit, second = %second_commit, result = %result, "Ancestor check");

    let result2 = git::is_ancestor(&second_commit, &first_commit, dir.path())?;
    info!(first = %second_commit, second = %first_commit, result = %result2, "Ancestor check");

    assert!(git::is_ancestor(&first_commit, &second_commit, dir.path())?);
    assert!(!git::is_ancestor(
        &second_commit,
        &first_commit,
        dir.path()
    )?);

    Ok(())
}

#[test]
fn test_git_bundle() -> Result<()> {
    init_test_logging();
    // Create source repository
    let source_dir = TempDir::new()?;
    init_git_repo(&source_dir)?;
    create_commit(&source_dir)?;

    // Create bundle
    let bundle_file = source_dir.path().join("test.bundle");
    git::bundle_create(&bundle_file, "HEAD", source_dir.path())?;

    // Verify bundle contents
    let verify = Command::new("git")
        .args(["bundle", "verify", bundle_file.to_str().unwrap()])
        .current_dir(&source_dir)
        .output()?;
    info!(
        output = %String::from_utf8_lossy(&verify.stderr),
        "Bundle verify output"
    );

    // Create target repository and unbundle
    let target_dir = TempDir::new()?;
    init_git_repo(&target_dir)?;

    // Create an initial commit in target repo
    create_commit(&target_dir)?;

    // Add the bundle as a remote
    Command::new("git")
        .args(["remote", "add", "origin", bundle_file.to_str().unwrap()])
        .current_dir(target_dir.path())
        .output()?;

    // Fetch from the bundle
    let fetch = Command::new("git")
        .args(["fetch", "origin"])
        .current_dir(target_dir.path())
        .output()?;
    info!(output = %String::from_utf8_lossy(&fetch.stderr), "Fetch output");

    // Checkout the fetched commit
    let checkout = Command::new("git")
        .args(["checkout", "FETCH_HEAD"])
        .current_dir(target_dir.path())
        .output()?;
    info!(
        output = %String::from_utf8_lossy(&checkout.stderr),
        "Checkout output"
    );

    // Verify commit exists in target
    let result = Command::new("git")
        .args(["log", "--oneline", "HEAD"])
        .current_dir(target_dir.path())
        .output()?;
    info!(output = %String::from_utf8_lossy(&result.stdout), "Log output");

    assert!(String::from_utf8_lossy(&result.stdout).contains("test commit"));

    Ok(())
}
