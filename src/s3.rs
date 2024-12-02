use std::path::Path;
use tracing::instrument;

use anyhow::{Context, Result};
use aws_config::{meta::region::RegionProviderChain, SdkConfig};
use aws_sdk_s3::{config::Builder as S3Builder, primitives::ByteStream, Client};
use aws_types::region::Region;

#[derive(Debug)]
pub struct Key {
    pub bucket: String,
    pub key: String,
}

/// Get an object from S3 and write it to a local file
#[instrument(skip(s3))]
pub async fn get(s3: &Client, f: &Path, o: &Key) -> Result<()> {
    let req = s3
        .get_object()
        .bucket(&o.bucket)
        .key(&o.key)
        .send()
        .await
        .with_context(|| format!("Failed to get object s3://{}/{}", o.bucket, o.key))?;

    let body = req.body;
    let bytes = body
        .collect()
        .await
        .context("Failed to collect object bytes from stream")?;

    std::fs::write(f, bytes.into_bytes())
        .with_context(|| format!("Failed to write object to file: {}", f.display()))?;

    Ok(())
}

/// Put a local file to S3
#[instrument(skip(s3))]
pub async fn put(s3: &Client, f: &Path, o: &Key) -> Result<()> {
    let contents =
        std::fs::read(f).with_context(|| format!("Failed to read file: {}", f.display()))?;

    let body = ByteStream::from(contents);

    s3.put_object()
        .bucket(&o.bucket)
        .key(&o.key)
        .body(body)
        .send()
        .await
        .with_context(|| format!("Failed to put object to s3://{}/{}", o.bucket, o.key))?;

    Ok(())
}

/// Delete an object from S3
#[instrument(skip(s3))]
pub async fn del(s3: &Client, o: &Key) -> Result<()> {
    s3.delete_object()
        .bucket(&o.bucket)
        .key(&o.key)
        .send()
        .await
        .with_context(|| format!("Failed to delete object s3://{}/{}", o.bucket, o.key))?;

    Ok(())
}

/// Rename an object in S3
#[instrument(skip(s3))]
pub async fn rename(s3: &Client, from: &Key, to: &Key) -> Result<()> {
    // Copy the object
    s3.copy_object()
        .copy_source(format!("{}/{}", from.bucket, from.key))
        .bucket(to.bucket.clone())
        .key(to.key.clone())
        .send()
        .await
        .with_context(|| {
            format!(
                "Failed to copy object from s3://{}/{} to s3://{}/{}",
                from.bucket, from.key, to.bucket, to.key
            )
        })?;

    // Delete the original
    del(s3, from).await?;

    Ok(())
}

/// Create an S3 client from AWS SDK configuration
pub fn create_client(config: &SdkConfig, force_path_style: bool) -> Client {
    let mut client_config = S3Builder::from(config);
    client_config.set_force_path_style(Some(force_path_style));
    Client::from_conf(client_config.build())
}

/// Create a region provider chain with a default region
pub fn create_region_provider(region: Option<String>) -> RegionProviderChain {
    RegionProviderChain::first_try(region.map(Region::new))
        .or_default_provider()
        .or_else(Region::new("us-east-1"))
}
