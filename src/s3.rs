use std::path::Path;
use tracing::instrument;

use anyhow::Result;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;

#[derive(Debug)]
pub struct Key {
    pub bucket: String,
    pub key: String,
}

#[instrument(skip(s3))]
pub async fn get(s3: &Client, f: &Path, o: &Key) -> Result<()> {
    let req = s3.get_object().bucket(&o.bucket).key(&o.key).send().await?;

    let body = req.body;
    let bytes = body.collect().await?;

    std::fs::write(f, bytes.into_bytes())?;

    Ok(())
}

#[instrument(skip(s3))]
pub async fn put(s3: &Client, f: &Path, o: &Key) -> Result<()> {
    let contents = std::fs::read(f)?;

    let body = ByteStream::from(contents);

    s3.put_object()
        .bucket(&o.bucket)
        .key(&o.key)
        .body(body)
        .send()
        .await?;

    Ok(())
}

#[instrument(skip(s3))]
pub async fn del(s3: &Client, o: &Key) -> Result<()> {
    s3.delete_object()
        .bucket(&o.bucket)
        .key(&o.key)
        .send()
        .await?;

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
