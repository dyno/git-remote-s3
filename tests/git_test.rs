use anyhow::Result;
use std::env::set_current_dir;
use std::fs;
use std::process::Command;
use tempfile::TempDir;
use tracing::info;

mod common;
use common::init_test_logging;

use git_remote_s3::git;

const TEST_EMAIL: &str = "test@example.com";
const TEST_USER: &str = "Test User";

fn init_git_repo() -> Result<TempDir> {
    // Create a new temp directory for the git repo
    let repo_dir = TempDir::new()?;
    info!("Created temp dir: {:?}", repo_dir.path());
    set_current_dir(&repo_dir)?;

    // Initialize git repo
    Command::new("git").arg("init").output()?;

    Command::new("git")
        .args(["config", "--local", "user.email", TEST_EMAIL])
        .output()?;

    Command::new("git")
        .args(["config", "--local", "user.name", TEST_USER])
        .output()?;

    Ok(repo_dir)
}

fn create_commits(repo_dir: &TempDir, num_commits: usize) -> Result<Vec<String>> {
    set_current_dir(&repo_dir)?;

    let mut commit_shas = Vec::new();

    for i in 1..=num_commits {
        let file_name = format!("test{}.txt", i);
        let test_file = repo_dir.path().join(&file_name);
        fs::write(&test_file, format!("test content {}", i))?;

        Command::new("git").args(["add", &file_name]).output()?;

        Command::new("git")
            .args(["commit", "-m", &format!("test commit {}", i)])
            .output()?;

        let output = Command::new("git").args(["rev-parse", "HEAD"]).output()?;
        let commit_sha = String::from_utf8(output.stdout)?.trim().to_string();
        commit_shas.push(commit_sha);
    }

    Ok(commit_shas)
}

fn create_commit(repo_dir: &TempDir) -> Result<String> {
    let commit_shas = create_commits(repo_dir, 1)?;
    Ok(commit_shas.into_iter().next().unwrap())
}

#[test]
fn test_git_rev_parse() -> Result<()> {
    init_test_logging();

    let repo_dir = init_git_repo()?;
    set_current_dir(&repo_dir)?;

    let head = create_commit(&repo_dir)?;

    assert!(!head.is_empty());
    assert_eq!(head.len(), 40); // SHA-1 hash is 40 characters

    Ok(())
}

#[test]
fn test_git_is_ancestor() -> Result<()> {
    init_test_logging();

    let repo_dir = init_git_repo()?;
    set_current_dir(repo_dir.path())?;

    let commits = create_commits(&repo_dir, 2)?;
    let first_commit = &commits[0];
    let second_commit = &commits[1];

    // Test ancestry using our git module
    assert!(git::is_ancestor(&first_commit, &second_commit)?);
    assert!(!git::is_ancestor(&second_commit, &first_commit)?);

    // Test with non-existent commits
    assert!(!git::is_ancestor("non-existent", &second_commit)?);
    assert!(!git::is_ancestor(&first_commit, "non-existent")?);

    Ok(())
}

#[test]
fn test_git_bundle() -> Result<()> {
    init_test_logging();

    let source_dir = init_git_repo()?;
    set_current_dir(&source_dir)?;

    // Create bundle
    create_commit(&source_dir)?;
    let bundle_file = source_dir.path().join("test.bundle");
    git::bundle_create(bundle_file.as_path(), "HEAD")?;

    // Verify bundle contents
    let output = Command::new("git")
        .args(["bundle", "verify", bundle_file.to_str().unwrap()])
        .output()?;
    assert!(output.status.success());

    // Initialize target repo and unbundle
    let target_dir = init_git_repo()?;
    set_current_dir(target_dir.path())?;

    git::bundle_unbundle(bundle_file.as_path(), "")?;

    Ok(())
}
