extern crate assert_cmd;

use aws_sdk_s3::{
    config::{Credentials, Region},
    types::BucketLocationConstraint,
    Client, Config,
};

use tempfile::Builder;

use assert_cmd::cargo::cargo_bin;
use assert_cmd::prelude::*;

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;
use std::error::Error;

const TEST_GPG_KEY: &str = "test@example.com";

fn setup_gpg(pwd: &Path) -> Result<(), Box<dyn Error>> {
    // Configure git to use test key
    Command::new("git")
        .current_dir(pwd)
        .args(&["config", "user.name", "Test User"])
        .output()?;
    
    Command::new("git")
        .current_dir(pwd)
        .args(&["config", "user.email", TEST_GPG_KEY])
        .output()?;
        
    Command::new("git")
        .current_dir(pwd)
        .args(&["config", "--add", "remote.origin.gpgRecipients", TEST_GPG_KEY])
        .output()?;
    
    Ok(())
}

fn git(pwd: &Path, args: &str) -> Command {
    let bin_path = cargo_bin("git-remote-s3");
    let my_path = bin_path.parent().unwrap().to_str().unwrap();
    let new_path = format!("{}:{}", my_path, env::var("PATH").unwrap());

    let mut command = Command::new("git");
    command.current_dir(pwd);
    command.env("PATH", new_path);
    command.env("S3_ENDPOINT", "http://localhost:9001");
    command.env("AWS_ACCESS_KEY_ID", "test");
    command.env("AWS_SECRET_ACCESS_KEY", "test1234");
    command.env("GIT_S3_ENCRYPT", "1");  // Enable GPG encryption
    cmd_args(&mut command, args);
    command
}

fn cmd_args(command: &mut Command, args: &str) {
    let words: Vec<_> = args.split_whitespace().collect();
    for word in words {
        command.arg(word);
    }
}

async fn create_test_client() -> Client {
    let creds = Credentials::new("test", "test1234", None, None, "test");
    let region = Region::new("us-east-1");
    
    let conf = Config::builder()
        .credentials_provider(creds)
        .endpoint_url("http://localhost:9001")
        .region(region)
        .force_path_style(true)
        .build();

    Client::from_conf(conf)
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
                .filter_map(|obj| obj.key().map(String::from))
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
                .location_constraint(BucketLocationConstraint::UsEast2)
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
    for k in keys {
        delete_object(client, bucket, &k).await?;
    }
    delete_bucket(client, bucket).await?;
    Ok(())
}

fn git_rev(pwd: &Path) -> String {
    let output = Command::new("git")
        .current_dir(pwd)
        .args(&["rev-parse", "--short", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn git_rev_long(pwd: &Path) -> String {
    let output = Command::new("git")
        .current_dir(pwd)
        .args(&["rev-parse", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[tokio::test]
async fn integration() {
    let test_dir = Builder::new()
        .prefix("git_s3_test")
        .tempdir()
        .expect("Failed to create temp dir");
    println!("Test dir: {}", test_dir.path().display());

    let client = create_test_client().await;
    
    // Ensure MinIO is ready by retrying bucket creation
    let mut retries = 5;
    loop {
        let result = delete_bucket_recurse(&client, "git-remote-s3").await;
        match result {
            Ok(_) => break,
            Err(_) if retries > 1 => {
                thread::sleep(Duration::from_secs(1));
                retries -= 1;
                continue;
            }
            Err(e) => {
                println!("Error deleting bucket (expected on first run): {:?}", e);
                break;
            }
        }
    }

    if let Err(e) = create_bucket(&client, "git-remote-s3").await {
        println!("Error creating bucket: {:?}", e);
    } else {
        println!("Created bucket git-remote-s3");
    }

    let repo1 = test_dir.path().join("repo1");
    let repo2 = test_dir.path().join("repo2");
    fs::create_dir(&repo1).unwrap();
    fs::create_dir(&repo2).unwrap();

    println!("test: pushing from repo1");
    git(&repo1, "init -b master").assert().success();
    setup_gpg(&repo1).expect("Failed to setup GPG");
    fs::write(repo1.join("test.txt"), "test").unwrap();
    git(&repo1, "add test.txt").assert().success();
    git(&repo1, "commit -m test").assert().success();
    
    // Remove remote if it exists
    let _ = Command::new("git")
        .current_dir(&repo1)
        .args(&["remote", "remove", "origin"])
        .output();
    
    git(&repo1, "remote add origin s3://git-remote-s3/test")
        .assert()
        .success();
    git(&repo1, "push origin master").assert().success();

    println!("test: cloning into repo2");
    git(&repo2, "clone s3://git-remote-s3/test .").assert().success();
    setup_gpg(&repo2).expect("Failed to setup GPG");
    assert_eq!(git_rev(&repo1), git_rev(&repo2));
    assert_eq!(git_rev_long(&repo1), git_rev_long(&repo2));

    // Clean up
    if let Err(e) = delete_bucket_recurse(&client, "git-remote-s3").await {
        println!("Error cleaning up bucket: {:?}", e);
    } else {
        println!("Cleaned up bucket git-remote-s3");
    }
}
