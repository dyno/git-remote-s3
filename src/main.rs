use anyhow::{anyhow, Result};
use aws_sdk_s3::Client;
use std::{
    env, io,
    path::{Path, PathBuf},
};
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;

mod common;
mod git;
mod git_s3;
mod gpg;
mod log;
mod s3;

use crate::git_s3::{fetch_from_s3, get_s3_client, list_refs, push_to_s3, GitRef, Settings};

// implemented the git-remote-helpers protocol: https://git-scm.com/docs/gitremote-helpers

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging to /tmp/git-remote-s3.log
    let log_path = PathBuf::from("/tmp/git-remote-s3.log");
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("error,git_remote_s3=debug"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(move || file.try_clone().unwrap())
        .event_format(log::GoogleEventFormat)
        .fmt_fields(log::GoogleFormatFields)
        .init();
    info!(message = "-".repeat(80));

    let mut args = env::args();
    args.next();
    let alias = args.next().ok_or_else(|| anyhow!("must provide alias"))?;
    let url = args.next().ok_or_else(|| anyhow!("must provide url"))?;

    info!(?alias, ?url, "Starting git-remote-s3");

    let url_path = url.trim_start_matches("s3://");
    let slash_idx = url_path
        .find('/')
        .ok_or_else(|| anyhow!("url must contain /"))?;
    let bucket = &url_path[..slash_idx];
    let path = &url_path[(slash_idx + 1)..];

    debug!(?bucket, ?path, "Parsed S3 URL");

    let s3_settings = Settings {
        remote_alias: env::var("REMOTE_ALIAS").unwrap_or_else(|_| alias.clone()),
        root: s3::Key {
            bucket: env::var("S3_BUCKET").unwrap_or_else(|_| bucket.to_string()),
            key: env::var("S3_KEY").unwrap_or_else(|_| path.to_string()),
        },
        endpoint: env::var("S3_ENDPOINT").ok(),
        region: env::var("AWS_REGION").ok(),
    };

    debug!(?s3_settings, "Created S3 settings");

    let s3 = get_s3_client(&s3_settings).await?;
    info!("S3 client initialized");

    let settings = Settings {
        remote_alias: alias,
        root: s3::Key {
            bucket: bucket.to_string(),
            key: path.to_string(),
        },
        endpoint: s3_settings.endpoint,
        region: s3_settings.region,
    };

    let current_dir = std::env::current_dir()?;

    match cmd_loop(&s3, &settings, &current_dir).await {
        Ok(_) => Ok(()),
        Err(e) => {
            error!(?e, "Command loop failed");
            Err(e)
        }
    }
}

async fn cmd_loop(s3: &Client, settings: &Settings, current_dir: &Path) -> Result<()> {
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
            (Some("push"), Some(ref_arg), None) => {
                cmd_push(s3, settings, ref_arg, current_dir).await
            }
            (Some("fetch"), Some(sha), Some(name)) => {
                cmd_fetch(s3, settings, sha, name, current_dir).await
            }
            (Some("capabilities"), None, None) => cmd_capabilities(),
            (Some("list"), None, None) => cmd_list(s3, settings).await,
            (Some("list"), Some("for-push"), None) => cmd_list(s3, settings).await,
            (None, None, None) => return Ok(()),
            _ => cmd_unknown(),
        };

        if let Err(e) = result {
            error!(?e, "Command execution failed");
            return Err(e);
        }
    }
}

fn cmd_unknown() -> Result<()> {
    println!("unknown command");
    println!();
    Ok(())
}

/// capabilities
/// Lists the capabilities of the helper, one per line, ending with a blank line.
/// Each capability may be preceded with *, which marks them mandatory for Git
/// versions using the remote helper to understand. Any unknown mandatory capability
/// is a fatal error.
///
/// Support for this command is mandatory.

fn cmd_capabilities() -> Result<()> {
    println!("*push");
    println!("*fetch");
    println!();
    Ok(())
}

/// list
/// Lists the refs, one per line, in the format "<value> <name> [<attr> …​]". The
/// value may be a hex sha1 hash, "@<dest>" for a symref, ":<keyword> <value>" for a
/// key-value pair, or "?" to indicate that the helper could not get the value of
/// the ref. A space-separated list of attributes follows the name; unrecognized
/// attributes are ignored. The list ends with a blank line.
///
/// See REF LIST ATTRIBUTES for a list of currently defined attributes. See REF LIST
/// KEYWORDS for a list of currently defined keywords.
///
/// Supported if the helper has the "fetch" or "import" capability.
///
/// list for-push
/// Similar to list, except that it is used if and only if the caller wants to the
/// resulting ref list to prepare push commands. A helper supporting both push and
/// fetch can use this to distinguish for which operation the output of list is
/// going to be used, possibly reducing the amount of work that needs to be
/// performed.
///
/// Supported if the helper has the "push" or "export" capability.

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

/// fetch <sha1> <name>
/// Fetches the given object, writing the necessary objects to the database. Fetch
/// commands are sent in a batch, one per line, terminated with a blank line.
/// Outputs a single blank line when all fetch commands in the same batch are
/// complete. Only objects which were reported in the output of list with a sha1 may
/// be fetched this way.
///
/// Optionally may output a lock <file> line indicating the full path of a file
/// under $GIT_DIR/objects/pack which is keeping a pack until refs can be suitably
/// updated. The path must end with .keep. This is a mechanism to name a
/// <pack,idx,keep> tuple by giving only the keep component. The kept pack will not
/// be deleted by a concurrent repack, even though its objects may not be referenced
/// until the fetch completes. The .keep file will be deleted at the conclusion of
/// the fetch.
///
/// If option check-connectivity is requested, the helper must output
/// connectivity-ok if the clone is self-contained and connected.
///
/// Supported if the helper has the "fetch" capability.

async fn cmd_fetch(
    s3: &Client,
    settings: &Settings,
    sha: &str,
    name: &str,
    current_dir: &Path,
) -> Result<()> {
    if name == "HEAD" {
        // Ignore head, as it's guaranteed to point to a ref we already downloaded
        return Ok(());
    }
    let git_ref = GitRef {
        name: name.to_string(),
        sha: sha.to_string(),
    };
    fetch_from_s3(s3, settings, &git_ref, current_dir).await?;
    println!();
    Ok(())
}

/// push +<src>:<dst>
/// Pushes the given local <src> commit or branch to the remote branch described by
/// <dst>. A batch sequence of one or more push commands is terminated with a blank
/// line (if there is only one reference to push, a single push command is followed
/// by a blank line). For example, the following would be two batches of push, the
/// first asking the remote-helper to push the local ref master to the remote ref
/// master and the local HEAD to the remote branch, and the second asking to push
/// ref foo to ref bar (forced update requested by the +).
///
/// push refs/heads/master:refs/heads/master
/// push HEAD:refs/heads/branch
/// \n
/// push +refs/heads/foo:refs/heads/bar
/// \n
/// Zero or more protocol options may be entered after the last push command, before
/// the batch’s terminating blank line.
///
/// When the push is complete, outputs one or more ok <dst> or error <dst> <why>?
/// lines to indicate success or failure of each pushed ref. The status report
/// output is terminated by a blank line. The option field <why> may be quoted in a
/// C style string if it contains an LF.
///
/// Supported if the helper has the "push" capability.

async fn cmd_push(
    s3: &Client,
    settings: &Settings,
    push_ref: &str,
    current_dir: &Path,
) -> Result<()> {
    let force = push_ref.starts_with('+');

    info!(?push_ref, force, "Pushing ref");

    // Parse src:dst refspec
    let (src_ref, dst_ref) = match push_ref.trim_start_matches('+').split_once(':') {
        Some((src, dst)) => (src, dst),
        None => (
            push_ref.trim_start_matches('+'),
            push_ref.trim_start_matches('+'),
        ),
    };

    // Get all remote refs
    let all_remote_refs = list_refs(s3, settings).await?;
    let remote_refs = all_remote_refs.get(src_ref);
    let prev_ref = remote_refs.map(|rs| rs.latest_ref());
    let local_sha = git::rev_parse(src_ref, current_dir)?;

    debug!(?local_sha, "Resolved local SHA");

    let local_ref = GitRef {
        name: dst_ref.to_string(),
        sha: local_sha,
    };

    // Check if we can push
    let can_push = force
        || match prev_ref {
            Some(prev_ref) => {
                if !git::is_ancestor(&prev_ref.reference.sha, &local_ref.sha, current_dir)? {
                    warn!(?dst_ref, "Remote changed - force push required");
                    println!("error {} remote changed: force push to add new ref, the old ref will be kept until its merged)", dst_ref);
                    false
                } else {
                    true
                }
            }
            None => true,
        };

    if can_push {
        info!(?local_ref, "Pushing to S3");
        push_to_s3(s3, settings, &local_ref, current_dir).await?;

        // Delete any ref that is an ancestor of the one we pushed
        for r in remote_refs.iter().flat_map(|r| r.by_update_time.iter()) {
            // Only delete refs with the same name
            if r.reference.name != local_ref.name {
                continue;
            }
            if git::is_ancestor(&r.reference.sha, &local_ref.sha, current_dir)? {
                debug!(?r.object, "Deleting old ref");
                s3::del(s3, &r.object).await?;
            }
        }

        println!("ok {}", dst_ref);
    } else if force {
        info!(?local_ref, "Force pushing to S3");
        push_to_s3(s3, settings, &local_ref, current_dir).await?;

        // On force push, we keep the old ref but rename it
        for r in remote_refs.iter().flat_map(|r| r.by_update_time.iter()) {
            // Only handle refs with the same name
            if r.reference.name != local_ref.name {
                continue;
            }

            // Don't touch refs that are ancestors
            if git::is_ancestor(&r.reference.sha, &local_ref.sha, current_dir)? {
                continue;
            }

            // Rename the ref to include its SHA
            let new_ref = GitRef {
                name: format!("{}_{}", r.reference.name, &r.reference.sha[..7]),
                sha: r.reference.sha.clone(),
            };
            let new_obj = s3::Key {
                bucket: settings.root.bucket.clone(),
                key: format!("refs/{}", new_ref.name),
            };
            info!(?new_ref, "Renaming old ref");
            s3::rename(
                s3,
                &s3::Key {
                    bucket: settings.root.bucket.clone(),
                    key: r.object.key.clone(),
                },
                &new_obj,
            )
            .await?;
        }

        println!("ok {}", dst_ref);
    } else {
        println!("error {} remote changed: force push to add new ref, the old ref will be kept until its merged)", dst_ref);
    }

    println!();
    Ok(())
}
