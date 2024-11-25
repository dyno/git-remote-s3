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
    command.env("GIT_S3_NO_ENCRYPT", "1");  // Skip GPG encryption
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

async fn delete_object(client: &Client, bucket: &str, filename: &str) {
    client
        .delete_object()
        .bucket(bucket)
        .key(filename)
        .send()
        .await
        .expect("Couldn't delete object");
}

async fn list_keys_in_bucket(client: &Client, bucket: &str) -> Vec<String> {
    match client.list_objects_v2().bucket(bucket).send().await {
        Ok(output) => {
            output
                .contents()
                .unwrap_or_default()
                .iter()
                .filter_map(|obj| obj.key().map(String::from))
                .collect()
        }
        _ => vec![],
    }
}

async fn create_bucket(client: &Client, bucket: &str) {
    client
        .create_bucket()
        .bucket(bucket)
        .create_bucket_configuration(
            aws_sdk_s3::types::CreateBucketConfiguration::builder()
                .location_constraint(BucketLocationConstraint::UsEast2)
                .build(),
        )
        .send()
        .await
        .expect("Couldn't create bucket");
}

async fn delete_bucket(client: &Client, bucket: &str) {
    if let Err(e) = client.delete_bucket().bucket(bucket).send().await {
        println!("Error deleting bucket: {:?}", e);
    }
}

async fn delete_bucket_recurse(client: &Client, bucket: &str) {
    let keys = list_keys_in_bucket(client, bucket).await;
    for k in keys {
        delete_object(client, bucket, &k).await;
    }
    delete_bucket(client, bucket).await;
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
    delete_bucket_recurse(&client, "git-remote-s3").await;
    create_bucket(&client, "git-remote-s3").await;

    let repo1 = test_dir.path().join("repo1");
    let repo2 = test_dir.path().join("repo2");
    fs::create_dir(&repo1).unwrap();
    fs::create_dir(&repo2).unwrap();

    println!("test: pushing from repo1");
    git(&repo1, "init").assert().success();
    fs::write(repo1.join("test.txt"), "test").unwrap();
    git(&repo1, "add test.txt").assert().success();
    git(&repo1, "commit -m test").assert().success();
    git(&repo1, "remote add origin s3://git-remote-s3/test")
        .assert()
        .success();
    git(&repo1, "push origin main").assert().success();

    println!("test: cloning into repo2");
    git(&repo2, "clone s3://git-remote-s3/test .").assert().success();
    assert_eq!(git_rev(&repo1), git_rev(&repo2));
    assert_eq!(git_rev_long(&repo1), git_rev_long(&repo2));

    delete_bucket_recurse(&client, "git-remote-s3").await;
}
