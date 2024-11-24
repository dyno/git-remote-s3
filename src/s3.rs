use aws_sdk_s3::Client;
use aws_sdk_s3::primitives::ByteStream;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use tokio::fs;

use super::errors::*;

#[derive(Debug, Clone)]
pub struct Key {
    pub bucket: String,
    pub key: String,
}

pub async fn get(client: &Client, o: &Key, f: &Path) -> Result<()> {
    let resp = client
        .get_object()
        .bucket(&o.bucket)
        .key(&o.key)
        .send()
        .await
        .map_err(|e| ErrorKind::S3Error(format!("failed to get object: {}", e)))?;

    let body = resp.body.collect().await
        .map_err(|e| ErrorKind::S3Error(format!("failed to read object body: {}", e)))?;

    fs::write(f, body.into_bytes())
        .await
        .map_err(|e| ErrorKind::S3Error(format!("failed to write file: {}", e)))?;

    Ok(())
}

pub async fn put(client: &Client, f: &Path, o: &Key) -> Result<()> {
    let mut file = File::open(f)
        .map_err(|e| ErrorKind::S3Error(format!("failed to open file: {}", e)))?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)
        .map_err(|e| ErrorKind::S3Error(format!("failed to read file: {}", e)))?;

    client
        .put_object()
        .bucket(&o.bucket)
        .key(&o.key)
        .body(ByteStream::from(contents))
        .send()
        .await
        .map_err(|e| ErrorKind::S3Error(format!("failed to put object: {}", e)))?;

    Ok(())
}

pub async fn del(client: &Client, o: &Key) -> Result<()> {
    client
        .delete_object()
        .bucket(&o.bucket)
        .key(&o.key)
        .send()
        .await
        .map_err(|e| ErrorKind::S3Error(format!("failed to delete object: {}", e)))?;

    Ok(())
}

pub async fn list(client: &Client, k: &Key) -> Result<Vec<Key>> {
    let resp = client
        .list_objects_v2()
        .bucket(&k.bucket)
        .prefix(&k.key)
        .send()
        .await
        .map_err(|e| ErrorKind::S3Error(format!("failed to list objects: {}", e)))?;

    let mut keys = Vec::new();
    if let Some(contents) = resp.contents {
        for object in contents {
            if let Some(key) = object.key {
                keys.push(Key {
                    bucket: k.bucket.clone(),
                    key,
                });
            }
        }
    }

    Ok(keys)
}
