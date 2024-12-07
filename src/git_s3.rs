use anyhow::{Result};
use aws_config::{retry::RetryConfig, timeout::TimeoutConfig};
use aws_sdk_s3::Client;
use once_cell::sync::OnceCell;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap},
    path::Path,
    time::Duration,
};
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
        self.endpoint
            .get_or_init(|| std::env::var("S3_ENDPOINT").ok())
            .as_deref()
    }

    pub fn region(&self) -> Option<&str> {
        self.region
            .get_or_init(|| std::env::var("AWS_REGION").ok())
            .as_deref()
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
    /// Last modified timestamp of the S3 object, used for sorting refs by update time.
    /// Stored as Unix timestamp in nanoseconds since epoch.
    ///
    /// # Example
    /// ```
    /// # use git_remote_s3::git_s3::{RemoteRef, GitRef};
    /// let remote_ref = RemoteRef {
    ///     updated: 1701925200_000_000_000, // Dec 7, 2023 00:00:00.000000000 UTC
    ///     reference: GitRef {
    ///         name: "main".to_string(),
    ///         sha: "abc123".to_string(),
    ///     },
    /// };
    /// ```
    pub updated: i128,
    pub reference: GitRef,
}

#[derive(Debug)]
pub struct RemoteRefs {
    // BTreeMap with Reverse ordering to sort timestamps in descending order (newest first)
    by_update_time: BTreeMap<Reverse<i128>, RemoteRef>,
}

impl RemoteRefs {
    pub fn new() -> Self {
        RemoteRefs {
            by_update_time: BTreeMap::new(),
        }
    }

    pub fn latest_ref(&self) -> &RemoteRef {
        // Get the first entry since we're using Reverse ordering
        self.by_update_time.values().next().unwrap()
    }

    pub fn add_ref(&mut self, remote_ref: RemoteRef) {
        let timestamp = Reverse(remote_ref.updated);
        self.by_update_time.insert(timestamp, remote_ref);
    }

    pub fn stale_refs(&self) -> impl Iterator<Item = &RemoteRef> {
        // Skip the first entry (most recent) and return the rest
        self.by_update_time.values().skip(1)
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

/// Lists all Git references stored in S3, organized by reference name.
///
/// retrieves all objects from the S3 bucket under the specified prefix and
/// organizes them into a map of Git references. Each entry in the map
/// represents a reference (e.g., "main", "feature/xyz") and contains all
/// versions of that reference sorted by their last modified timestamp.

pub async fn list_refs(
    s3: &Client,
    settings: &GitS3Settings,
) -> Result<HashMap<String, RemoteRefs>> {
    let result = s3
        .list_objects_v2()
        .bucket(settings.bucket())
        .prefix(settings.key())
        .send()
        .await?;

    let objects = result.contents().unwrap_or_default();

    // Parse S3 keys into RemoteRefs
    let refs_with_names = objects.iter().filter_map(|obj| {
        // key = project1.git/refs/heads/features/fXXX/99d98906d65894a9eac5fda27b0c41d2cf372dd6.bundle
        let key = obj.key()?;
        let mut parts = key.trim_end_matches(".bundle").rsplit('/');
        let sha = parts.next()?; // sha = 99d98906d65894a9eac5fda27b0c41d2cf372dd6
        
        // name = refs/heads/features/fXXX
        let name = key
            .strip_prefix(settings.key())? // Remove prefix (e.g. "project1.git")
            .trim_start_matches('/')
            .strip_suffix(&format!("/{}.bundle", sha))? // Remove suffix (e.g. "/[sha].bundle")
            .to_string();

        Some((
            name.clone(),
            RemoteRef {
                updated: obj
                    .last_modified()
                    .map(|dt| dt.as_nanos())
                    .unwrap_or_default(),
                reference: GitRef {
                    name,
                    sha: sha.to_string(),
                },
            },
        ))
    });

    // Group refs by name into a HashMap
    let refs_map = refs_with_names.fold(HashMap::new(), |mut acc, (name, remote_ref)| {
        acc.entry(name)
            .or_insert_with(RemoteRefs::new)
            .add_ref(remote_ref);
        acc
    });

    Ok(refs_map)
}

// Git bundle operations
pub async fn fetch_from_s3(
    s3: &Client,
    settings: &GitS3Settings,
    r: &GitRef,
    current_dir: &Path,
) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_refs_sorting() {
        let mut refs = RemoteRefs::new();

        // Add refs with different timestamps (nanoseconds)
        refs.add_ref(RemoteRef {
            updated: 1701925200_000_000_000, // Dec 7, 2023 00:00:00.000000000 UTC
            reference: GitRef {
                name: "main".to_string(),
                sha: "abc123".to_string(),
            },
        });

        refs.add_ref(RemoteRef {
            updated: 1701838800_000_000_000, // Dec 6, 2023 00:00:00.000000000 UTC
            reference: GitRef {
                name: "main".to_string(),
                sha: "def456".to_string(),
            },
        });

        // Verify that latest_ref returns the most recent ref
        let latest = refs.latest_ref();
        assert_eq!(latest.updated, 1701925200_000_000_000);
        assert_eq!(latest.reference.sha, "abc123");

        // Verify that stale_refs returns older refs in order
        let stale: Vec<_> = refs.stale_refs().collect();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].updated, 1701838800_000_000_000);
        assert_eq!(stale[0].reference.sha, "def456");
    }
}
