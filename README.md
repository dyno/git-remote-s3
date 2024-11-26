# Git Remote S3 Helper

A Git remote helper that enables pushing and pulling git repositories to/from an S3 bucket.
Uses gpg to encrypt the repo contents (but not branch names!) before sending to s3.

This is most useful for small teams who don't want to host their own
private repository, but still want to manage their own encryption.
For example, my use case is periodically backing up a repo from a desktop
and pull to a laptop to develop remotely.

## Features

- Push/pull git repositories to/from S3 buckets
- GPG encryption for repository contents
- AWS SDK for Rust integration
- Support for custom endpoints (e.g., MinIO)
- Configurable AWS region and credentials

## Example Usage

Add a remote using the `s3` transport:
```bash
git remote add s3remote s3://my_bucket/prefix
```

And then you can push/pull to the remote as usual:
```bash
git pull s3remote main

git push s3remote
```

Or even clone from s3:
```bash
git clone s3://my_bucket/prefix
```

## Installation

1. Install the binary:
   * Download the latest release [here](https://github.com/dyno/git-remote-s3/releases/latest), gunzip and put it in your PATH
   * Or, install using cargo: `cargo install git-remote-s3`

2. Configure AWS credentials:
   * Set up AWS credentials using any of the standard methods (environment variables, credentials file, etc.)
   * Required environment variables:
     ```bash
     AWS_ACCESS_KEY_ID=your_access_key
     AWS_SECRET_ACCESS_KEY=your_secret_key
     ```
   * Optional environment variables:
     ```bash
     AWS_REGION=your_region         # defaults to us-east-1
     S3_ENDPOINT=your_endpoint_url  # for custom endpoints like MinIO
     ```

3. Setup GPG (Optional but recommended):
   * GPG encryption is enabled by default (GIT_S3_ENCRYPT=1)
   * The system will use `git config user.email` as the GPG recipient
   * Ensure you have public and private keys setup for this user
   * Alternatively, set specific recipients using `git config --add remote.<name>.gpgRecipients "user1@example.com user2@example.com"`
   * To disable encryption: `export GIT_S3_ENCRYPT=0`

## Development

### Prerequisites
* Rust 1.82 or later
* Docker (for MinIO in tests)
* GPG

### Building
```bash
# Local build
cargo build

# Cross-platform build using Docker
make cross-build-with-docker
```

### Testing
```bash
# Start MinIO (required for tests)
make start-minio

# In another terminal, run tests
make test
```

The test suite will:
1. Set up a test GPG key if not present
2. Start a local MinIO instance
3. Run integration tests that verify:
   * Basic push/pull operations
   * Force push behavior
   * Multiple head handling
   * GPG encryption/decryption

## Design Notes

The semantics of pushing are slightly different from a 'proper' git repository:

* Non-force pushes require the current head as an ancestor
* Multiple heads can exist for the same branch
  * The newest head is considered the truth
  * Older heads use the naming scheme: `<branch_name>__<sha>`
  * View all heads using `git ls-remote`
* Old heads are retained until a new head includes them as ancestors
* Each branch is stored on S3 as: `s3://bucket/prefix/<ref_name>/<sha>.bundle`
  * Files are bundled with `git bundle` and encrypted with `gpg`
  * Average operations:
    * `git push`: 2 list, 1 put, 1 delete
    * `git pull`: 1 list, 1 get

## Future Improvements

* Better notification of multiple heads on S3
  * Show warning when attempting push/fetch with multiple heads
* Use `gpg.program` configuration
* Performance optimizations for large repositories

## Acknowledgements

- Fork of [git-remote-s3](https://github.com/bgahagan/git-remote-s3/)
- Updated with [Windsurf](https://codeium.com/windsurf)
- https://github.com/awslabs/git-remote-s3, "This library enables to use Amazon S3 as a git remote and LFS server."
