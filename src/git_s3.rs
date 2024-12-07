use anyhow::{anyhow, Result};
use aws_config::{retry::RetryConfig, timeout::TimeoutConfig};
use aws_sdk_s3::Client;
use once_cell::sync::OnceCell;
use std::{collections::HashMap, path::Path, time::Duration};
use tracing::{debug, info};

use crate::{git, gpg, s3};

#[derive(Debug)]
pub struct GitS3Settings {
    // Provided properties
    pub remote_alias: String,
    pub url: String,
    
    // Derived properties
    bucket: OnceCell<String>,
    key: OnceCell<String>,
    endpoint: OnceCell<Option<String>>,
    region: OnceCell<Option<String>>,
}

impl GitS3Settings {
    pub fn new(remote_alias: String, url: String) -> Self {
        assert!(url.starts_with("s3://"));
        GitS3Settings {
            remote_alias,
            url,
            bucket: OnceCell::new(),
            key: OnceCell::new(),
            endpoint: OnceCell::new(),
            region: OnceCell::new(),
        }
    }

    pub fn bucket(&self) -> &str {
        self.bucket.get_or_init(|| {
            let path = self.url.strip_prefix("s3://").unwrap();
            path.split_once('/').unwrap().0.to_string()
        })
    }

    pub fn key(&self) -> &str {
        self.key.get_or_init(|| {
            let path = self.url.strip_prefix("s3://").unwrap();
            path.split_once('/').unwrap().1.to_string()
        })
    }

    pub fn endpoint(&self) -> Option<&str> {
        self.endpoint.get_or_init(|| {
            std::env::var("S3_ENDPOINT").ok()
        }).as_deref()
    }

    pub fn region(&self) -> Option<&str> {
        self.region.get_or_init(|| {
            std::env::var("AWS_REGION").ok()
        }).as_deref()
    }
}

#[derive(Debug)]
pub struct GitRef {
    pub name: String,
    pub sha: String,
}

impl GitRef {
    fn bundle_path(&self, prefix: &str) -> String {
        format!("{}/{}/{}.bundle", prefix, self.name, self.sha)
    }
}

#[derive(Debug)]
pub struct RemoteRef {
    pub object: s3::Key,
    pub updated: String,
    pub reference: GitRef,
}

#[derive(Debug)]
pub struct RemoteRefs {
    pub by_update_time: Vec<RemoteRef>,
}

impl RemoteRefs {
    pub fn latest_ref(&self) -> &RemoteRef {
        self.by_update_time.get(0).unwrap()
    }
}

// S3 client configuration
pub async fn get_s3_client(settings: &GitS3Settings) -> Result<Client> {
    let region_provider = s3::create_region_provider(settings.region().map(String::from));

    let mut config_builder = aws_config::from_env()
        .region(region_provider)
        .retry_config(RetryConfig::standard().with_max_attempts(3))
        .timeout_config(
            TimeoutConfig::builder()
                .operation_timeout(Duration::from_secs(30))
                .build(),
        );

    if let Some(endpoint) = settings.endpoint() {
        config_builder = config_builder.endpoint_url(endpoint);
    }

    let config = config_builder.load().await;
    Ok(s3::create_client(&config, true))
}

// Basic S3 operations
pub async fn fetch(s3: &Client, o: &s3::Key, enc_file: &Path) -> Result<()> {
    s3::get(s3, &enc_file, o).await
}

pub async fn push(s3: &Client, enc_file: &Path, o: &s3::Key) -> Result<()> {
    s3::put(s3, enc_file, o).await
}

// Git reference operations
pub async fn list_refs(s3: &Client, settings: &GitS3Settings) -> Result<HashMap<String, RemoteRefs>> {
    let result = s3
        .list_objects_v2()
        .bucket(settings.bucket())
        .prefix(settings.key())
        .send()
        .await
        .map_err(|e| anyhow!("list objects failed: {}", e))?;

    let objects = result.contents().unwrap_or_default();
    let mut refs_map = HashMap::new();

    for obj in objects {
        if let Some(key) = obj.key() {
            let key_str = key.to_string();
            if let Some(last_slash) = key_str.rfind('/') {
                if let Some(last_dot) = key_str.rfind('.') {
                    let name = key_str
                        .get((settings.key().len() + 1)..last_slash)
                        .unwrap_or("")
                        .to_string();
                    let sha = key_str
                        .get((last_slash + 1)..last_dot)
                        .unwrap_or("")
                        .to_string();

                    let remote_ref = RemoteRef {
                        object: s3::Key {
                            bucket: settings.bucket().to_string(),
                            key: key_str,
                        },
                        updated: obj
                            .last_modified()
                            .map(|dt| dt.as_secs_f64().to_string())
                            .unwrap_or_default(),
                        reference: GitRef {
                            name: name.clone(),
                            sha,
                        },
                    };

                    refs_map
                        .entry(name)
                        .or_insert_with(|| RemoteRefs {
                            by_update_time: Vec::new(),
                        })
                        .by_update_time
                        .push(remote_ref);
                }
            }
        }
    }

    // Sort refs by update time
    for refs in refs_map.values_mut() {
        refs.by_update_time.sort_by(|a, b| b.updated.cmp(&a.updated));
    }

    Ok(refs_map)
}

// Git bundle operations
pub async fn fetch_from_s3(s3: &Client, settings: &GitS3Settings, r: &GitRef, current_dir: &Path) -> Result<()> {
    info!(?r, "Fetching from S3");

    let tmp_dir = std::env::temp_dir();
    debug!(?tmp_dir, "Created temporary directory");

    let bundle_file = tmp_dir.join("bundle");
    let enc_file = tmp_dir.join("bundle_enc");

    let path = r.bundle_path(settings.key());
    let o = s3::Key {
        bucket: settings.bucket().to_owned(),
        key: path,
    };

    debug!(?o, "Fetching bundle from S3");
    fetch(s3, &o, &enc_file).await?;

    debug!("Decrypting bundle");
    gpg::decrypt(&enc_file, &bundle_file)?;

    info!(?r.name, "Unbundling Git bundle");
    git::bundle_unbundle(&bundle_file, &r.name, current_dir)?;

    Ok(())
}

pub async fn push_to_s3(
    s3: &Client,
    settings: &GitS3Settings,
    r: &GitRef,
    current_dir: &Path,
) -> Result<()> {
    let tmp_dir = std::env::temp_dir();
    let bundle_file = tmp_dir.join("bundle");
    let enc_file = tmp_dir.join("bundle_enc");

    git::bundle_create(&bundle_file, &r.name, current_dir)?;

    let recipients = git::config(
        &format!("remote.{}.gpgRecipients", settings.remote_alias),
        current_dir,
    )
    .map(|config| {
        config
            .split_ascii_whitespace()
            .map(|s| s.to_string())
            .collect()
    })
    .or_else(|_| git::config("user.email", current_dir).map(|recip| vec![recip]))?;

    gpg::encrypt(&recipients, &bundle_file, &enc_file)?;

    let path = r.bundle_path(settings.key());
    let o = s3::Key {
        bucket: settings.bucket().to_owned(),
        key: path,
    };

    push(s3, &enc_file, &o).await?;

    Ok(())
}
