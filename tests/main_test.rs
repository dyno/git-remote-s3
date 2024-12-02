use assert_cmd::cargo::cargo_bin;
use assert_cmd::prelude::*;
use aws_config;
use aws_sdk_s3::{
    config::{Credentials, Region},
    Client,
};
use git_remote_s3::log::GoogleEventFormat;
use std::process::Command;
use std::{env, fs, path::Path, path::PathBuf, sync::Once};
use tempfile::Builder;
use tokio;
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;
use anyhow::Result;

const TEST_REGION: &str = "us-east-1";
const TEST_ENDPOINT: &str = "http://localhost:9001";
const TEST_ACCESS_KEY: &str = "test";
const TEST_SECRET_KEY: &str = "test1234";

static INIT_LOGGER: Once = Once::new();

fn setup() -> PathBuf {
    INIT_LOGGER.call_once(|| {
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("error,git_remote_s3=debug,main_test=debug"));

        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .event_format(GoogleEventFormat)
            .init();

        debug!("Logging initialized for main_test");
    });

    // Create a temporary directory for testing
    debug!("Setting up test environment");
    let test_dir = Builder::new().prefix("git_s3_test").tempdir().unwrap();
    debug!("Created test directory: {:?}", test_dir.path());
    test_dir.into_path()
}

fn git(pwd: &Path, args: &str) -> Command {
    let bin_path = cargo_bin("git-remote-s3");
    let parent_path = bin_path.parent().unwrap().to_str().unwrap();
    let new_path = format!("{}:{}", parent_path, env::var("PATH").unwrap());

    let mut command = Command::new("git");
    command.current_dir(pwd);
    command.env("PATH", new_path);
    command.env("S3_ENDPOINT", TEST_ENDPOINT);
    command.env("AWS_ACCESS_KEY_ID", TEST_ACCESS_KEY);
    command.env("AWS_SECRET_ACCESS_KEY", TEST_SECRET_KEY);
    cmd_args(&mut command, args);
    command
}

fn cmd_args(command: &mut Command, args: &str) {
    command.args(args.split_whitespace());
}

async fn create_test_client() -> Result<Client> {
    let config = aws_config::from_env()
        .region(Region::new(TEST_REGION))
        .endpoint_url(TEST_ENDPOINT)
        .credentials_provider(Credentials::new(
            TEST_ACCESS_KEY,
            TEST_SECRET_KEY,
            None,
            None,
            "test",
        ))
        .load()
        .await;

    let s3_config = aws_sdk_s3::config::Builder::from(&config)
        .force_path_style(true)
        .build();

    Ok(Client::from_conf(s3_config))
}

async fn delete_object(
    client: &Client,
    bucket: &str,
    filename: &str,
) -> Result<()> {
    client
        .delete_object()
        .bucket(bucket)
        .key(filename)
        .send()
        .await?;
    Ok(())
}

async fn list_keys_in_bucket(client: &Client, bucket: &str) -> Result<Vec<String>> {
    match client.list_objects_v2().bucket(bucket).send().await {
        Ok(output) => Ok(output
            .contents()
            .unwrap_or_default()
            .iter()
            .filter_map(|obj| Some(obj.key().unwrap_or_default().to_string()))
            .collect()),
        Err(e) => Err(e.into()),
    }
}

async fn create_bucket(client: &Client, bucket: &str) -> Result<()> {
    client
        .create_bucket()
        .bucket(bucket)
        .create_bucket_configuration(
            aws_sdk_s3::types::CreateBucketConfiguration::builder()
                .location_constraint(aws_sdk_s3::types::BucketLocationConstraint::UsEast2)
                .build(),
        )
        .send()
        .await?;
    Ok(())
}

async fn delete_bucket(client: &Client, bucket: &str) -> Result<()> {
    client.delete_bucket().bucket(bucket).send().await?;
    Ok(())
}

async fn delete_bucket_recurse(client: &Client, bucket: &str) -> Result<()> {
    let keys = list_keys_in_bucket(client, bucket).await?;
    for key in keys {
        delete_object(client, bucket, &key).await?;
    }
    delete_bucket(client, bucket).await?;
    Ok(())
}

fn git_rev(pwd: &Path) -> String {
    let out = git(pwd, "rev-parse --short HEAD").output().unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn git_rev_long(pwd: &Path) -> String {
    let out = git(pwd, "rev-parse HEAD").output().unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[tokio::test]
async fn integration() -> Result<()> {
    debug!("Starting integration test");
    let client = create_test_client().await?;
    debug!("Created S3 test client");
    let bucket = "git-remote-s3";

    // Setup s3 bucket
    let _ = delete_bucket_recurse(&client, bucket).await;
    create_bucket(&client, bucket).await?;
    debug!("Created test bucket: {}", bucket);

    let test_dir = setup();
    info!("Starting integration test");

    let repo1 = test_dir.join("repo1");
    let repo2 = test_dir.join("repo2");

    fs::create_dir(&repo1).unwrap();
    fs::create_dir(&repo2).unwrap();

    info!(repo = %repo1.display(), "Initializing first repository");
    git(&repo1, "init").assert().success();
    debug!("Initialized git repository");
    git(&repo1, "config user.email test@example.com")
        .assert()
        .success();
    git(&repo1, "config user.name Test").assert().success();
    git(&repo1, "branch -M main").assert().success();
    git(&repo1, "commit --allow-empty -am r1_c1")
        .assert()
        .success();
    debug!("Created initial commit");
    info!("test: pushing from repo1");
    git(&repo1, "remote add origin s3://git-remote-s3/test")
        .assert()
        .success();
    git(&repo1, "push --set-upstream origin main")
        .assert()
        .success();
    let _sha = git_rev(&repo1);

    info!("test: cloning into repo2");
    fs::create_dir_all(&repo2).unwrap();
    git(&repo2, "init").assert().success();
    git(&repo2, "config user.email test@example.com")
        .assert()
        .success();
    git(&repo2, "config user.name Test").assert().success();
    git(&repo2, "remote add origin s3://git-remote-s3/test")
        .assert()
        .success();
    git(&repo2, "fetch origin").assert().success();
    git(&repo2, "checkout main").assert().success();
    git(&repo2, "log --oneline --decorate=short")
        .assert()
        .success();

    info!("test: push from repo2 and pull into repo1");
    git(&repo2, "commit --allow-empty -am r2_c1")
        .assert()
        .success();
    git(&repo2, "push origin").assert().success();
    let sha = git_rev(&repo2);
    git(&repo1, "pull origin main").assert().success();
    git(&repo1, "log --oneline --decorate=short -n 1")
        .assert()
        .stdout(format!("{} (HEAD -> main, origin/main) r2_c1\n", sha));

    info!("test: force push from repo2");
    git(&repo1, "commit --allow-empty -am r1_c2")
        .assert()
        .success();
    git(&repo1, "push origin").assert().success();
    let sha1 = git_rev(&repo1);
    let sha1l = git_rev_long(&repo1);
    git(&repo2, "commit --allow-empty -am r2_c2")
        .assert()
        .success();
    let sha2 = git_rev(&repo2);
    let sha2l = git_rev_long(&repo2);
    git(&repo2, "push origin").assert().failure();
    git(&repo2, "push -f origin").assert().success();
    // assert that there are 2 refs on s3 (the original was kept)
    let ls_remote_output = git(&repo1, "ls-remote origin")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let ls_remote_str = String::from_utf8_lossy(&ls_remote_output);
    assert!(ls_remote_str.contains(&format!("{}\trefs/heads/main", sha2l)));
    assert!(ls_remote_str.contains(&format!("{}\trefs/heads/main__{}", sha1l, sha1)));

    git(&repo1, "pull -r origin main").assert().success();
    git(
        &repo1,
        format!("log --oneline --decorate=short -n 1 {}", sha2).as_str(),
    )
    .assert()
    .stdout(format!("{} (HEAD -> main, origin/main) r2_c2\n", sha2));
    git(&repo1, "push origin main").assert().success();
    // assert that refs are unchanged on s3
    let ls_remote_output = git(&repo1, "ls-remote origin")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let ls_remote_str = String::from_utf8_lossy(&ls_remote_output);
    assert!(ls_remote_str.contains(&format!("{}\trefs/heads/main", sha2l)));
    assert!(ls_remote_str.contains(&format!("{}\trefs/heads/main__{}", sha1l, sha1)));

    // Cleanup
    delete_bucket_recurse(&client, bucket).await?;
    info!("Test cleanup complete");

    Ok(())
}
