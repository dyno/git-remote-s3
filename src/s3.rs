use aws_sdk_s3::Client;
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
        
        std::fs::write(f, bytes.into_bytes())
            .chain_err(|| "write failed")?;
            
        Ok(())
    })
}

pub fn put(s3: &Client, f: &Path, o: &Key) -> Result<()> {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let contents = std::fs::read(f)
            .chain_err(|| "read failed")?;
        
        let body = aws_sdk_s3::primitives::ByteStream::from(contents);
        
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
