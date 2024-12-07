use anyhow::{anyhow, Result};
use aws_sdk_s3::Client;
use std::{env, io, path::PathBuf};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

mod git;
mod git_s3;
mod gpg;
mod log;
mod s3;

use crate::git_s3::{fetch_from_s3, list_refs, push_to_s3, GitRef, GitS3Settings};
use crate::s3::create_client;

// implemented the git-remote-helpers protocol: https://git-scm.com/docs/gitremote-helpers

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("error,git_remote_s3=info"));

    // Initialize logging to /tmp/git-remote-s3.log
    let log_path = PathBuf::from("/tmp/git-remote-s3.log");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(move || file.try_clone().unwrap())
        .event_format(log::GoogleEventFormat)
        .fmt_fields(log::GoogleFormatFields)
        .init();

    info!(message = "-".repeat(80));

    let mut args = env::args();
    let helper = args.next().unwrap();
    let alias = args.next().ok_or_else(|| anyhow!("must provide alias"))?;
    let url = args.next().ok_or_else(|| anyhow!("must provide url"))?;
    info!(?helper, ?alias, ?url, "Starting ");

    let settings = GitS3Settings::new(alias, url);
    let s3 = create_client(
        settings.region().map(String::from),
        settings.endpoint().map(String::from),
    )
    .await?;
    info!("S3 client initialized");

    match cmd_loop(&s3, &settings).await {
        Ok(_) => Ok(()),
        Err(e) => {
            error!(?e, "Command loop failed");
            Err(e)
        }
    }
}

async fn cmd_loop(s3: &Client, settings: &GitS3Settings) -> Result<()> {
    loop {
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

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

async fn cmd_list(s3: &Client, settings: &GitS3Settings) -> Result<()> {
    let refs = list_refs(s3, settings).await?;
    if !refs.is_empty() {
        for (_, refs) in refs.iter() {
            let latest = refs.latest_ref();
            println!("{} {}", latest.reference.sha, latest.reference.name);

            for stale_ref in refs.stale_refs() {
                let short_sha = &stale_ref.reference.sha[..7];
                println!(
                    "{} {}__{}",
                    stale_ref.reference.sha, stale_ref.reference.name, short_sha
                );
            }
        }

        if refs.contains_key("refs/heads/main") {
            println!("@refs/heads/main HEAD");
        } else if refs.contains_key("refs/heads/master") {
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

async fn cmd_fetch(s3: &Client, settings: &GitS3Settings, sha: &str, name: &str) -> Result<()> {
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

async fn cmd_push(s3: &Client, settings: &GitS3Settings, push_ref: &str) -> Result<()> {
    let force = push_ref.starts_with('+');
    let (src, dst) = match push_ref.trim_start_matches('+').split_once(':') {
        Some((src, dst)) => (src, dst),
        None => (
            push_ref.trim_start_matches('+'),
            push_ref.trim_start_matches('+'),
        ),
    };

    let current_dir = env::current_dir()?;
    let local_ref = GitRef {
        name: dst.to_string(),
        sha: git::rev_parse(src, &current_dir)?,
    };

    let refs = list_refs(s3, settings).await?;
    if !force {
        if let Some(prev_refs) = refs.get(&local_ref.name) {
            let prev_ref = prev_refs.latest_ref();
            if !prev_ref.reference.sha.is_empty() {
                if !git::is_ancestor(&prev_ref.reference.sha, &local_ref.sha, &current_dir)? {
                    warn!(?dst, "Remote changed - force push required");
                    println!("error {} remote changed: force push required", dst);
                    return Ok(());
                }
            }
        }
    }

    push_to_s3(s3, settings, &local_ref).await?;
    println!("ok {}", dst);
    println!();
    Ok(())
}
