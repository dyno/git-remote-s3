#![recursion_limit = "1024"]

use anyhow::{Result, anyhow, bail};
use aws_sdk_s3::Client;
use aws_config::meta::region::RegionProviderChain;
use aws_config::retry::RetryConfig;
use aws_config::timeout::TimeoutConfig;
use aws_types::region::Region;
use std::collections::HashMap;
use std::env;
use std::io;
use std::path::Path;
use std::time::Duration;
use tempfile::Builder;

mod errors;
mod git;
mod gpg;
mod s3;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = env::args();
    args.next();
    let alias = args.next().ok_or_else(|| anyhow!("must provide alias"))?;
    let url = args.next().ok_or_else(|| anyhow!("must provide url"))?;

    let url_path = url.trim_start_matches("s3://");
    let slash_idx = url_path.find('/').ok_or_else(|| anyhow!("url must contain /"))?;
    let bucket = &url_path[..slash_idx];
    let path = &url_path[(slash_idx + 1)..];

    let s3_settings = Settings {
        remote_alias: env::var("REMOTE_ALIAS").unwrap_or_else(|_| alias.clone()),
        root: s3::Key {
            bucket: env::var("S3_BUCKET").unwrap_or_else(|_| bucket.to_string()),
            key: env::var("S3_KEY").unwrap_or_else(|_| path.to_string()),
        },
        endpoint: env::var("S3_ENDPOINT").ok(),
        region: env::var("AWS_REGION").ok(),
    };

    let s3 = get_s3_client(&s3_settings).await?;

    let settings = Settings {
        remote_alias: alias,
        root: s3::Key {
            bucket: bucket.to_string(),
            key: path.to_string(),
        },
        endpoint: s3_settings.endpoint,
        region: s3_settings.region,
    };

    match cmd_loop(&s3, &settings).await {
        Ok(_) => Ok(()),
        Err(e) => {
            if errors::is_broken_pipe(&e) {
                // Exit gracefully on broken pipe
                std::process::exit(0);
            }
            eprintln!("Error: {}", e);
            Err(e)
        }
    }
}

async fn get_s3_client(settings: &Settings) -> Result<Client> {
    let region_provider = RegionProviderChain::first_try(settings.region.clone().map(Region::new))
        .or_default_provider()
        .or_else(Region::new("us-east-1"));

    let mut config_builder = aws_config::from_env()
        .region(region_provider)
        .retry_config(RetryConfig::standard().with_max_attempts(3))
        .timeout_config(TimeoutConfig::builder()
            .operation_timeout(Duration::from_secs(30))
            .build());

    if let Some(endpoint) = &settings.endpoint {
        config_builder = config_builder.endpoint_url(endpoint);
    }

    let config = config_builder.load().await;
    let mut client_config = aws_sdk_s3::config::Builder::from(&config);
    client_config.set_force_path_style(Some(true));
    Ok(Client::from_conf(client_config.build()))
}

#[derive(Debug)]
pub struct GitRef {
    pub name: String,
    pub sha: String,
}

impl GitRef {
    fn bundle_path(&self, prefix: String) -> String {
        format!("{}/{}/{}.bundle", prefix, self.name, self.sha)
    }
}

#[derive(Debug)]
pub struct RemoteRef {
    pub object: s3::Key,
    pub updated: String,
    pub reference: GitRef,
}

pub struct RemoteRefs {
    by_update_time: Vec<RemoteRef>,
}

impl RemoteRefs {
    fn latest_ref(&self) -> &RemoteRef {
        self.by_update_time.get(0).unwrap()
    }
}

async fn fetch(s3: &Client, o: &s3::Key, enc_file: &Path) -> Result<()> {
    s3::get(s3, &enc_file, o).await
}

async fn push(s3: &Client, enc_file: &Path, o: &s3::Key) -> Result<()> {
    s3::put(s3, enc_file, o).await
}

async fn list_refs(s3: &Client, settings: &Settings) -> Result<HashMap<String, RemoteRefs>> {
    let result = s3.list_objects_v2()
        .bucket(&settings.root.bucket)
        .prefix(&settings.root.key)
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
                        .get((settings.root.key.len() + 1)..last_slash)
                        .unwrap_or("")
                        .to_string();
                    let sha = key_str
                        .get((last_slash + 1)..last_dot)
                        .unwrap_or("")
                        .to_string();

                    let remote_ref = RemoteRef {
                        object: s3::Key {
                            bucket: settings.root.bucket.clone(),
                            key: key_str,
                        },
                        updated: obj.last_modified()
                            .map(|dt| dt.as_secs_f64().to_string())
                            .unwrap_or_default(),
                        reference: GitRef { name: name.clone(), sha },
                    };

                    refs_map.entry(name)
                        .or_insert_with(|| RemoteRefs { by_update_time: Vec::new() })
                        .by_update_time.push(remote_ref);
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

async fn cmd_list(s3: &Client, settings: &Settings) -> Result<()> {
    let refs = list_refs(s3, settings).await?;
    if !refs.is_empty() {
        for (_, refs) in refs.iter() {
            let latest = refs.latest_ref();
            println!("{} {}", latest.reference.sha, latest.reference.name);

            for stale_ref in refs.by_update_time.iter().skip(1) {
                let short_sha = &stale_ref.reference.sha[..7];
                println!(
                    "{} {}__{}",
                    stale_ref.reference.sha, stale_ref.reference.name, short_sha
                );
            }
        }

        if refs.contains_key("refs/heads/master") {
            println!("@refs/heads/master HEAD");
        }
    }
    println!();
    Ok(())
}

async fn cmd_loop(s3: &Client, settings: &Settings) -> Result<()> {
    loop {
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| anyhow!("read error: {}", e))?;

        if input.is_empty() {
            return Ok(());
        }

        let mut iter = input.split_ascii_whitespace();
        let cmd = iter.next();
        let arg1 = iter.next();
        let arg2 = iter.next();

        let result = match (cmd, arg1, arg2) {
            (Some("push"), Some(ref_arg), None) => cmd_push(s3, settings, ref_arg).await,
            (Some("fetch"), Some(sha), Some(name)) => cmd_fetch(s3, settings, sha, name).await,
            (Some("capabilities"), None, None) => cmd_capabilities(),
            (Some("list"), None, None) => cmd_list(s3, settings).await,
            (Some("list"), Some("for-push"), None) => cmd_list(s3, settings).await,
            (None, None, None) => return Ok(()),
            _ => cmd_unknown(),
        };

        if let Err(e) = result {
            if errors::is_broken_pipe(&e) {
                return Ok(());  // Exit gracefully on broken pipe
            }
            return Err(e);
        }
    }
}

fn cmd_unknown() -> Result<()> {
    println!("unknown command");
    println!();
    Ok(())
}

fn cmd_capabilities() -> Result<()> {
    println!("*push");
    println!("*fetch");
    println!();
    Ok(())
}

async fn fetch_from_s3(s3: &Client, settings: &Settings, r: &GitRef) -> Result<()> {
    let tmp_dir = Builder::new()
        .prefix("s3_fetch")
        .tempdir()
        .map_err(|e| anyhow!("mktemp dir failed: {}", e))?;
    let bundle_file = tmp_dir.path().join("bundle");
    let enc_file = tmp_dir.path().join("buncle_enc");

    let path = r.bundle_path(settings.root.key.to_owned());
    let o = s3::Key {
        bucket: settings.root.bucket.to_owned(),
        key: path,
    };
    fetch(s3, &o, &enc_file).await?;

    gpg::decrypt(&enc_file, &bundle_file)?;

    git::bundle_unbundle(&bundle_file, &r.name)?;

    Ok(())
}

async fn push_to_s3(s3: &Client, settings: &Settings, r: &GitRef) -> Result<()> {
    let tmp_dir = Builder::new()
        .prefix("s3_push")
        .tempdir()
        .map_err(|e| anyhow!("mktemp dir failed: {}", e))?;
    let bundle_file = tmp_dir.path().join("bundle");
    let enc_file = tmp_dir.path().join("buncle_enc");

    git::bundle_create(&bundle_file, &r.name)?;

    let recipients = git::config(&format!("remote.{}.gpgRecipients", settings.remote_alias))
        .map(|config| {
            config
                .split_ascii_whitespace()
                .map(|s| s.to_string())
                .collect()
        })
        .or_else(|_| git::config("user.email").map(|recip| vec![recip]))?;

    gpg::encrypt(&recipients, &bundle_file, &enc_file)?;

    let path = r.bundle_path(settings.root.key.to_owned());
    let o = s3::Key {
        bucket: settings.root.bucket.to_owned(),
        key: path,
    };
    push(s3, &enc_file, &o).await?;

    Ok(())
}

async fn cmd_fetch(s3: &Client, settings: &Settings, sha: &str, name: &str) -> Result<()> {
    if name == "HEAD" {
        // Ignore head, as it's guaranteed to point to a ref we already downloaded
        return Ok(());
    }
    let git_ref = GitRef {
        name: name.to_string(),
        sha: sha.to_string(),
    };
    fetch_from_s3(s3, settings, &git_ref).await?;
    println!();
    Ok(())
}

async fn cmd_push(s3: &Client, settings: &Settings, push_ref: &str) -> Result<()> {
    let force = push_ref.starts_with('+');

    let mut split = push_ref.split(':');

    let src_ref = split.next().unwrap();
    let src_ref = if force { &src_ref[1..] } else { src_ref };
    let dst_ref = split.next().unwrap();

    if src_ref != dst_ref {
        bail!("src_ref != dst_ref")
    }

    let all_remote_refs = list_refs(s3, settings).await?;
    let remote_refs = all_remote_refs.get(src_ref);
    let prev_ref = remote_refs.map(|rs| rs.latest_ref());
    let local_sha = git::rev_parse(src_ref)?;
    let local_ref = GitRef {
        name: src_ref.to_string(),
        sha: local_sha,
    };

    let can_push = force
        || match prev_ref {
            Some(prev_ref) => {
                if !git::is_ancestor(&local_ref.sha, &prev_ref.reference.sha)? {
                    println!("error {} remote changed: force push to add new ref, the old ref will be kept until its merged)", dst_ref);
                    false
                } else {
                    true
                }
            }
            None => true,
        };

    if can_push {
        push_to_s3(s3, settings, &local_ref).await?;

        // Delete any ref that is an ancestor of the one we pushed
        for r in remote_refs.iter().flat_map(|r| r.by_update_time.iter()) {
            if git::is_ancestor(&local_ref.sha, &r.reference.sha)? {
                s3::del(s3, &r.object).await?;
            }
        }

        println!("ok {}", dst_ref);
    };

    println!();
    Ok(())
}

pub struct Settings {
    remote_alias: String,
    root: s3::Key,
    endpoint: Option<String>,
    region: Option<String>,
}
