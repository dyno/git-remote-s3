use aws_sdk_s3::{
    Client,
    primitives::ByteStream,
};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use super::errors::*;

#[derive(Debug)]
pub struct Key {
    pub bucket: String,
    pub key: String,
}

pub fn get(s3: &Client, o: &Key, f: &Path) -> Result<()> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let req = s3.get_object()
            .bucket(&o.bucket)
            .key(&o.key)
            .send()
            .await
            .chain_err(|| "couldn't get item")?;

        let body = req.body;
        let bytes = body.collect().await.chain_err(|| "failed to collect body")?;
        
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(f)
            .chain_err(|| "open failed")?;
            
        file.write_all(&bytes.into_bytes())
            .chain_err(|| "write failed")?;
            
        Ok(())
    })
}

pub fn put(s3: &Client, f: &Path, o: &Key) -> Result<()> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let mut file = File::open(f).chain_err(|| "open failed")?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).chain_err(|| "read failed")?;
        
        let body = ByteStream::from(contents);
        
        s3.put_object()
            .bucket(&o.bucket)
            .key(&o.key)
            .body(body)
            .send()
            .await
            .chain_err(|| "Couldn't PUT object")?;
            
        Ok(())
    })
}

pub fn del(s3: &Client, o: &Key) -> Result<()> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        s3.delete_object()
            .bucket(&o.bucket)
            .key(&o.key)
            .send()
            .await
            .chain_err(|| "Couldn't DELETE object")?;
            
        Ok(())
    })
}

pub struct ListObjectsOutput {
    pub contents: Option<Vec<Key>>,
}

pub fn list(s3: &Client, k: &Key) -> Result<ListObjectsOutput> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let resp = s3.list_objects_v2()
            .bucket(&k.bucket)
            .prefix(&k.key)
            .send()
            .await
            .chain_err(|| "Couldn't list items in bucket")?;
            
        let contents = resp.contents().unwrap_or_default();
        let keys = contents.iter().map(|obj| Key {
            bucket: k.bucket.clone(),
            key: obj.key().unwrap_or_default().to_string(),
        }).collect();
        
        Ok(ListObjectsOutput {
            contents: Some(keys)
        })
    })
}
