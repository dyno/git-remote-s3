#[allow(dead_code)]
use anyhow::{anyhow, Result};
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::Client;
use std::path::Path;
use tracing::{debug, error};

#[derive(Debug)]
#[allow(dead_code)]
pub struct Key {
    pub bucket: String,
    pub key: String,
}

#[allow(dead_code)]
pub async fn get(s3: &Client, f: &Path, o: &Key) -> Result<()> {
    debug!(?o, ?f, "Getting object from S3");

    let req = s3
        .get_object()
        .bucket(&o.bucket)
        .key(&o.key)
        .send()
        .await
        .map_err(|e| match e {
            SdkError::ServiceError(se) => match se.err() {
                GetObjectError::NoSuchKey(_) => {
                    error!(?o, "Key not found in S3");
                    anyhow!("Key not found")
                }
                _ => {
                    error!(?se, "S3 service error");
                    anyhow!("S3 error: {}", se.err())
                }
            },
            _ => {
                error!(?e, "AWS SDK error");
                anyhow!("AWS error: {}", e)
            }
        })?;

    let body = req.body;
    debug!("Collecting response body");
    let bytes = body.collect().await.map_err(|e| {
        error!(?e, "Failed to collect response body");
        anyhow!("Failed to collect body: {}", e)
    })?;

    debug!(?f, "Writing file");
    std::fs::write(f, bytes.into_bytes()).map_err(|e| {
        error!(?e, ?f, "Failed to write file");
        anyhow!("Failed to write file: {}", e)
    })?;

    debug!("Successfully downloaded object");
    Ok(())
}

#[allow(dead_code)]
pub async fn put(s3: &Client, f: &Path, o: &Key) -> Result<()> {
    debug!(?o, ?f, "Putting object to S3");

    let contents = std::fs::read(f).map_err(|e| {
        error!(?e, ?f, "Failed to read file");
        anyhow!("Failed to read file: {}", e)
    })?;

    let body = aws_sdk_s3::primitives::ByteStream::from(contents);

    s3.put_object()
        .bucket(&o.bucket)
        .key(&o.key)
        .body(body)
        .send()
        .await
        .map_err(|e| match e {
            SdkError::ServiceError(se) => {
                error!(?se, "S3 service error");
                anyhow!("S3 error: {}", se.err())
            }
            _ => {
                error!(?e, "AWS SDK error");
                anyhow!("AWS error: {}", e)
            }
        })?;

    debug!("Successfully uploaded object");
    Ok(())
}

#[allow(dead_code)]
pub async fn del(s3: &Client, o: &Key) -> Result<()> {
    debug!(?o, "Deleting object from S3");

    s3.delete_object()
        .bucket(&o.bucket)
        .key(&o.key)
        .send()
        .await
        .map_err(|e| match e {
            SdkError::ServiceError(se) => {
                error!(?se, "S3 service error");
                anyhow!("S3 error: {}", se.err())
            }
            _ => {
                error!(?e, "AWS SDK error");
                anyhow!("AWS error: {}", e)
            }
        })?;

    debug!("Successfully deleted object");
    Ok(())
}
