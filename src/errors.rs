use error_chain::error_chain;

error_chain! {
    foreign_links {
        Io(::std::io::Error);
        S3(aws_sdk_s3::Error);
    }

    errors {
        InvalidUrl(url: String) {
            description("invalid url")
            display("invalid url: {}", url)
        }
        BucketNotFound(bucket: String) {
            description("bucket not found")
            display("bucket not found: {}", bucket)
        }
        GitError(details: String) {
            description("git operation failed")
            display("git error: {}", details)
        }
        S3Error(details: String) {
            description("s3 operation failed")
            display("s3 error: {}", details)
        }
        GpgError(details: String) {
            description("gpg operation failed")
            display("gpg error: {}", details)
        }
    }
}
