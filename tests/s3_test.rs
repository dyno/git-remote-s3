use anyhow::Result;
use aws_sdk_s3::{
    config::{Credentials, Region},
    Client,
};
use std::fs;
use std::io::Write;
use tempfile::NamedTempFile;

mod common;
use common::init_test_logging;

use git_remote_s3::s3::{self, Key};

const TEST_REGION: &str = "us-east-1";
const TEST_ENDPOINT: &str = "http://localhost:9001";
const TEST_ACCESS_KEY: &str = "test";
const TEST_SECRET_KEY: &str = "test1234";
const TEST_BUCKET: &str = "git-remote-s3";

async fn ensure_test_bucket(s3: &Client) -> Result<()> {
    match s3.create_bucket().bucket(TEST_BUCKET).send().await {
        Ok(_) => Ok(()),
        Err(e) => {
            if e.to_string().contains("BucketAlreadyOwnedByYou") {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Failed to create bucket: {}", e))
            }
        }
    }
}

#[tokio::test]
async fn test_s3_operations() -> Result<()> {
    init_test_logging();
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

    let s3 = s3::create_client(&config, true);
    ensure_test_bucket(&s3).await?;

    // Create a test file
    let mut input_file = NamedTempFile::new()?;
    write!(input_file, "test content")?;

    // Create a temporary file for download
    let output_file = NamedTempFile::new()?;

    // Test put
    let key = Key {
        bucket: TEST_BUCKET.to_string(),
        key: "test".to_string(),
    };
    s3::put(&s3, input_file.path(), &key).await?;

    // Test get
    s3::get(&s3, output_file.path(), &key).await?;

    // Verify content
    let content = fs::read_to_string(output_file.path())?;
    assert_eq!(content, "test content");

    // Test delete
    s3::del(&s3, &key).await?;

    Ok(())
}
