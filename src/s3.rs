use std::path::Path;
use tracing::instrument;

use anyhow::{anyhow, Result};
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::Client;

#[derive(Debug)]
pub struct Key {
    pub bucket: String,
    pub key: String,
}

#[instrument(skip(s3))]
pub async fn get(s3: &Client, f: &Path, o: &Key) -> Result<()> {
    let req = s3
        .get_object()
        .bucket(&o.bucket)
        .key(&o.key)
        .send()
        .await
        .map_err(|e| match e {
            SdkError::ServiceError(se) => match se.err() {
                GetObjectError::NoSuchKey(_) => anyhow!("Key not found"),
                _ => anyhow!("S3 error: {}", se.err()),
            },
            _ => anyhow!("AWS error: {}", e),
        })?;

    let body = req.body;
    let bytes = body
        .collect()
        .await
        .map_err(|e| anyhow!("Failed to collect body: {}", e))?;

    std::fs::write(f, bytes.into_bytes()).map_err(|e| anyhow!("Failed to write file: {}", e))?;

    Ok(())
}

#[instrument(skip(s3))]
pub async fn put(s3: &Client, f: &Path, o: &Key) -> Result<()> {
    let contents = std::fs::read(f).map_err(|e| anyhow!("Failed to read file: {}", e))?;

    let body = aws_sdk_s3::primitives::ByteStream::from(contents);

    s3.put_object()
        .bucket(&o.bucket)
        .key(&o.key)
        .body(body)
        .send()
        .await
        .map_err(|e| match e {
            SdkError::ServiceError(se) => anyhow!("S3 error: {}", se.err()),
            _ => anyhow!("AWS error: {}", e),
        })?;

    Ok(())
}

#[instrument(skip(s3))]
pub async fn del(s3: &Client, o: &Key) -> Result<()> {
    s3.delete_object()
        .bucket(&o.bucket)
        .key(&o.key)
        .send()
        .await
        .map_err(|e| match e {
            SdkError::ServiceError(se) => anyhow!("S3 error: {}", se.err()),
            _ => anyhow!("AWS error: {}", e),
        })?;

    Ok(())
}

#[instrument(skip(s3))]
pub async fn rename(s3: &Client, from: &Key, to: &Key) -> Result<()> {
    // Copy the object
    s3.copy_object()
        .copy_source(format!("{}/{}", from.bucket, from.key))
        .bucket(to.bucket.clone())
        .key(to.key.clone())
        .send()
        .await?;

    // Delete the original
    del(s3, from).await?;

    Ok(())
}
