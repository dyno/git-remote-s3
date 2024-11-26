use aws_sdk_s3::{
    Client,
    config::Credentials,
};
use aws_config;
use aws_types::region::Region;
use assert_cmd::cargo::cargo_bin;
use assert_cmd::prelude::*;
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::Builder;
use tracing_subscriber::fmt;
use std::fs::OpenOptions;

fn setup() -> PathBuf {
    // Enable debug logging only for our crate
    std::env::set_var("RUST_LOG", "git_remote_s3=debug");
    
    // Initialize logging to file
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/git-remote-s3.log")
        .unwrap();

    // Initialize logging to file only, without ANSI colors
    fmt()
        .with_env_filter("git_remote_s3=debug")
        .with_writer(file)
        .with_ansi(false)
        .init();
    
    let test_dir = Builder::new()
        .prefix("git_s3_test")
        .tempdir()
        .unwrap()
        .into_path();
    println!("Test dir: {}", test_dir.display());
    test_dir
}

fn git(pwd: &Path, args: &str) -> Command {
    let my_path = cargo_bin("git-remote-s3");
    let my_path = my_path.parent().unwrap();
    let my_path = my_path.to_str().unwrap();
    let new_path = format!("{}:{}", my_path, env::var("PATH").unwrap());

    let mut command = Command::new("git");
    command.current_dir(pwd);
    command.env("PATH", new_path);
    command.env("S3_ENDPOINT", "http://localhost:9001");
    command.env("AWS_ACCESS_KEY_ID", "test");
    command.env("AWS_SECRET_ACCESS_KEY", "test1234");
    cmd_args(&mut command, args);
    command
}

fn cmd_args(command: &mut Command, args: &str) {
    let words: Vec<_> = args.split_whitespace().collect();
    for word in words {
        command.arg(word);
    }
}

async fn create_test_client() -> Result<Client, Box<dyn Error>> {
    let config = aws_config::from_env()
        .region(Region::new("us-east-1"))
        .endpoint_url("http://localhost:9001")
        .credentials_provider(Credentials::new(
            "test",
            "test1234",
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

async fn delete_object(client: &Client, bucket: &str, filename: &str) -> Result<(), Box<dyn Error>> {
    client
        .delete_object()
        .bucket(bucket)
        .key(filename)
        .send()
        .await?;
    Ok(())
}

async fn list_keys_in_bucket(client: &Client, bucket: &str) -> Result<Vec<String>, Box<dyn Error>> {
    match client.list_objects_v2().bucket(bucket).send().await {
        Ok(output) => {
            Ok(output
                .contents()
                .unwrap_or_default()
                .iter()
                .filter_map(|obj| Some(obj.key().unwrap_or_default().to_string()))
                .collect())
        }
        Err(e) => Err(e.into()),
    }
}

async fn create_bucket(client: &Client, bucket: &str) -> Result<(), Box<dyn Error>> {
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

async fn delete_bucket(client: &Client, bucket: &str) -> Result<(), Box<dyn Error>> {
    client.delete_bucket().bucket(bucket).send().await?;
    Ok(())
}

async fn delete_bucket_recurse(client: &Client, bucket: &str) -> Result<(), Box<dyn Error>> {
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
async fn integration() -> Result<(), Box<dyn Error>> {
    let client = create_test_client().await?;
    let bucket = "git-remote-s3";

    // Setup s3 bucket
    let _ = delete_bucket_recurse(&client, bucket).await;
    create_bucket(&client, bucket).await?;

    let test_dir = setup();

    let repo1 = test_dir.join("repo1");
    let repo2 = test_dir.join("repo2");

    fs::create_dir(&repo1).unwrap();
    fs::create_dir(&repo2).unwrap();

    println!("test: pushing from repo1");
    git(&repo1, "init").assert().success();
    git(&repo1, "config user.email test@example.com").assert().success();
    git(&repo1, "config user.name Test").assert().success();
    git(&repo1, "branch -M main").assert().success();
    git(&repo1, "commit --allow-empty -am r1_c1")
        .assert()
        .success();
    git(&repo1, "remote add origin s3://git-remote-s3/test")
        .assert()
        .success();
    git(&repo1, "push --set-upstream origin main").assert().success();
    let _sha = git_rev(&repo1);

    println!("test: cloning into repo2");
    git(&repo2, "clone s3://git-remote-s3/test .")
        .assert()
        .success();
    git(&repo2, "config user.email test@example.com").assert().success();
    git(&repo2, "config user.name Test").assert().success();
    git(&repo2, "log --oneline --decorate=short")
        .assert()
        .success();

    println!("test: push from repo2 and pull into repo1");
    git(&repo2, "commit --allow-empty -am r2_c1")
        .assert()
        .success();
    git(&repo2, "push origin").assert().success();
    let sha = git_rev(&repo2);
    git(&repo1, "pull origin main").assert().success();
    git(&repo1, "log --oneline --decorate=short -n 1")
        .assert()
        .stdout(format!("{} (HEAD -> main, origin/main) r2_c1\n", sha));

    println!("test: force push from repo2");
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

    Ok(())
}