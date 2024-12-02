use std::fs;
use std::io::Write;
use std::path::PathBuf;

use anyhow::Result;
use aws_config::timeout::TimeoutConfig;
use aws_sdk_s3::Client;
use aws_types::region::Region;
use tempfile::NamedTempFile;

use git_remote_s3::s3::{self, Key};

const TEST_REGION: &str = "us-east-1";
const TEST_ENDPOINT: &str = "http://localhost:9001";
const TEST_BUCKET: &str = "test-bucket";

async fn setup_test_client() -> Client {
    let timeout = TimeoutConfig::builder()
        .connect_timeout(std::time::Duration::from_secs(1))
        .build();

    let config = aws_config::from_env()
        .region(Region::new(TEST_REGION))
        .endpoint_url(TEST_ENDPOINT)
        .timeout_config(timeout)
        .load()
        .await;

    Client::new(&config)
}

async fn ensure_test_bucket(s3: &Client) -> Result<()> {
    match s3.create_bucket().bucket(TEST_BUCKET).send().await {
        Ok(_) => Ok(()),
        Err(e) => {
            // Ignore BucketAlreadyExists error
            if e.to_string().contains("BucketAlreadyExists") {
                Ok(())
            } else {
                Err(e.into())
            }
        }
    }
}

#[tokio::test]
async fn test_s3_operations() -> Result<()> {
    // Skip if localstack is not running
    if !is_localstack_running().await {
        println!("Skipping S3 test as localstack is not running");
        return Ok(());
    }

    let s3 = setup_test_client().await;
    ensure_test_bucket(&s3).await?;

    // Create a test file
    let mut input_file = NamedTempFile::new()?;
    write!(input_file, "test content")?;
    
    // Create a temporary file for download
    let output_file = NamedTempFile::new()?;

    // Create test key
    let key = Key {
        bucket: TEST_BUCKET.to_string(),
        key: "test-key".to_string(),
    };

    // Test put operation
    s3::put(&s3, input_file.path(), &key).await?;

    // Test get operation
    s3::get(&s3, output_file.path(), &key).await?;

    // Verify content
    let content = fs::read_to_string(output_file.path())?;
    assert_eq!(content, "test content");

    // Test delete operation
    s3::del(&s3, &key).await?;

    // Verify delete worked by trying to get the file again
    let result = s3::get(&s3, &output_file.path(), &key).await;
    assert!(result.is_err());

    Ok(())
}

async fn is_localstack_running() -> bool {
    let client = reqwest::Client::new();
    match client.get(TEST_ENDPOINT).send().await {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}
