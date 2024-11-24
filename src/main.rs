#![recursion_limit = "1024"]
#[macro_use]
extern crate error_chain;
extern crate tokio;
extern crate log;
extern crate env_logger;

use aws_sdk_s3::Client;
use tempfile::Builder;

use std::collections::HashMap;
use std::io;
use log::debug;

pub mod errors;
use errors::*;

mod git;
mod gpg;
mod s3;

#[tokio::main]
async fn main() -> Result<()> {
    if let Err(ref e) = run().await {
        println!("error: {}", e);

        for e in e.iter().skip(1) {
            println!("caused by: {}", e);
        }

        if let Some(backtrace) = e.backtrace() {
            println!("backtrace: {:?}", backtrace);
        }

        std::process::exit(1);
    }
    Ok(())
}

struct Settings {
    root: s3::Key,
}

impl Settings {
    fn from_url(url: &str) -> Result<Self> {
        let mut parts = url.split("://");
        let scheme = parts.next().ok_or("no scheme")?;
        let rest = parts.next().ok_or("no rest")?;

        if scheme != "s3" {
            bail!("unsupported scheme: {}", scheme);
        }

        let mut parts = rest.split('/');
        let bucket = parts.next().ok_or("no bucket")?;
        let path = parts.collect::<Vec<_>>().join("/");

        Ok(Settings {
            root: s3::Key {
                bucket: bucket.to_string(),
                key: path,
            },
        })
    }
}

async fn run() -> Result<()> {
    env_logger::init();

    let mut lines = io::stdin().lines();
    let first_line = lines.next().ok_or("no first line")?.map_err(|e| format!("failed to read first line: {}", e))?;
    let settings = Settings::from_url(&first_line)?;

    let config = aws_config::from_env().load().await;
    let client = Client::new(&config);

    while let Some(line) = lines.next() {
        let line = line.map_err(|e| format!("failed to read line: {}", e))?;
        debug!("received line: {}", line);

        let mut parts = line.split_whitespace();
        let cmd = parts.next().ok_or("no command")?;

        match cmd {
            "capabilities" => {
                println!("*push");
                println!("*fetch");
                println!();
            }
            "list" => {
                let _for_push = parts.next().ok_or("no for-push")? == "for-push";
                list_refs(&client, &settings).await?;
            }
            "push" => {
                let src_ref = parts.next().ok_or("no src-ref")?;
                let dst_ref = parts.next().ok_or("no dst-ref")?;
                let force = parts.next().map_or(false, |s| s == "+");

                if src_ref == "" {
                    println!("ok {}", dst_ref);
                    continue;
                }

                push_to_s3(&client, &settings, src_ref, dst_ref, force).await?;
            }
            "fetch" => {
                let sha = parts.next().ok_or("no sha")?;
                let fetch_ref = parts.next().ok_or("no fetch-ref")?;

                fetch_from_s3(&client, &settings, &GitRef { name: fetch_ref.to_string(), sha: sha.to_string() }).await?;
            }
            "" => {
                println!();
                break;
            }
            _ => bail!("unknown command: {}", cmd),
        }
    }

    Ok(())
}

async fn list_refs(client: &Client, settings: &Settings) -> Result<()> {
    let refs = list_remote_refs(client, settings).await?;
    for (name, refs) in refs {
        if let Some(r) = refs.latest_ref() {
            println!("{} {}", r.reference.sha, name);
        }
    }
    println!();
    Ok(())
}

async fn fetch_from_s3(client: &Client, settings: &Settings, r: &GitRef) -> Result<()> {
    let bundle_file = Builder::new().prefix("bundle").suffix(".bundle").tempfile()?;
    let enc_file = Builder::new().prefix("bundle").suffix(".bundle.gpg").tempfile()?;

    let object = s3::Key {
        bucket: settings.root.bucket.clone(),
        key: format!("{}/{}", settings.root.key, r.sha),
    };

    s3::get(client, &object, enc_file.path()).await?;
    gpg::decrypt(enc_file.path(), bundle_file.path())?;
    git::bundle_unbundle(bundle_file.path(), &r.name)?;

    println!("ok {}", r.name);
    println!();
    Ok(())
}

async fn push_to_s3(client: &Client, settings: &Settings, src_ref: &str, dst_ref: &str, force: bool) -> Result<()> {
    let bundle_file = Builder::new().prefix("bundle").suffix(".bundle").tempfile()?;
    let enc_file = Builder::new().prefix("bundle").suffix(".bundle.gpg").tempfile()?;

    let local_sha = git::rev_parse(src_ref)?;

    let refs = list_remote_refs(client, settings).await?;
    let remote_refs = refs.get(dst_ref);
    let prev_ref = remote_refs.map(|rs| rs.latest_ref());

    if let Some(prev_ref) = prev_ref {
        if !force && !git::is_ancestor(&prev_ref.unwrap().reference.sha, &local_sha)? {
            println!("error {} non-fast-forward", dst_ref);
            s3::del(client, &prev_ref.unwrap().object).await?;
            return Ok(());
        }
    }

    git::bundle_create(bundle_file.path(), src_ref)?;

    let recipients = git::config("user.email").map(|recip| vec![recip])?;

    gpg::encrypt(&recipients, bundle_file.path(), enc_file.path())?;

    let object = s3::Key {
        bucket: settings.root.bucket.clone(),
        key: format!("{}/{}", settings.root.key, local_sha),
    };

    s3::put(client, enc_file.path(), &object).await?;

    println!("ok {}", dst_ref);
    println!();
    Ok(())
}

#[derive(Debug)]
struct GitRef {
    name: String,
    sha: String,
}

#[derive(Debug)]
struct RemoteRef {
    reference: GitRef,
    object: s3::Key,
    update_time: i64,
}

impl RemoteRef {
    fn from_key(k: &s3::Key, settings: &Settings) -> Option<(String, RemoteRef)> {
        if let Some(last_slash) = k.key.rfind('/') {
            let name = k.key.get((settings.root.key.len() + 1)..last_slash)?;
            let sha = k.key.get((last_slash + 1)..k.key.len())?;
            let name = name.to_string();
            let sha = sha.to_string();
            let remote_ref = RemoteRef {
                reference: GitRef { 
                    name: name.clone(), 
                    sha 
                },
                object: s3::Key {
                    bucket: k.bucket.clone(),
                    key: k.key.clone(),
                },
                update_time: 0,
            };
            Some((name, remote_ref))
        } else {
            None
        }
    }
}

#[derive(Debug)]
struct RemoteRefs {
    by_update_time: Vec<RemoteRef>,
}

impl RemoteRefs {
    fn new() -> Self {
        RemoteRefs {
            by_update_time: Vec::new(),
        }
    }

    fn add(&mut self, r: RemoteRef) {
        self.by_update_time.push(r);
        self.by_update_time.sort_by_key(|r| -r.update_time);
    }

    fn latest_ref(&self) -> Option<&RemoteRef> {
        self.by_update_time.get(0)
    }
}

async fn list_remote_refs(client: &Client, settings: &Settings) -> Result<HashMap<String, RemoteRefs>> {
    let objects = s3::list(client, &settings.root).await?;

    let map: HashMap<String, Vec<RemoteRef>> = objects
        .into_iter()
        .filter_map(|k| RemoteRef::from_key(&k, settings))
        .fold(HashMap::new(), |mut map, (name, r)| {
            map.entry(name).or_insert_with(Vec::new).push(r);
            map
        });

    Ok(map.into_iter()
        .map(|(name, mut refs)| {
            refs.sort_by_key(|r| -r.update_time);
            let mut remote_refs = RemoteRefs::new();
            for r in refs {
                remote_refs.add(r);
            }
            (name, remote_refs)
        })
        .collect())
}
