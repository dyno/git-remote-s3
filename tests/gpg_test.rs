use anyhow::Result;
use std::fs;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_gpg_no_recipients() -> Result<()> {
    // Create a temporary input file with some content
    let mut input_file = NamedTempFile::new()?;
    write!(input_file, "test content")?;

    // Create a temporary output file
    let output_file = NamedTempFile::new()?;

    // Test encryption with no recipients (should just copy)
    git_remote_s3::gpg::encrypt(&[], input_file.path(), output_file.path())?;

    // Verify content was copied correctly
    let content = fs::read_to_string(output_file.path())?;
    assert_eq!(content, "test content");

    Ok(())
}

#[test]
fn test_gpg_with_recipients() -> Result<()> {
    // Create a temporary input file with some content
    let mut input_file = NamedTempFile::new()?;
    write!(input_file, "secret content")?;

    // Create temporary files for encrypted and decrypted content
    let encrypted_file = NamedTempFile::new()?;
    let decrypted_file = NamedTempFile::new()?;

    // Get the first GPG key for testing
    let recipients = vec![get_test_gpg_key()?];

    // Encrypt the file
    git_remote_s3::gpg::encrypt(&recipients, input_file.path(), encrypted_file.path())?;

    // Decrypt the file
    git_remote_s3::gpg::decrypt(encrypted_file.path(), decrypted_file.path())?;

    // Verify decrypted content matches original
    let content = fs::read_to_string(decrypted_file.path())?;
    assert_eq!(content, "secret content");

    Ok(())
}

#[test]
fn test_gpg_missing_file() -> Result<()> {
    let nonexistent = tempfile::Builder::new()
        .prefix("nonexistent")
        .tempfile()?
        .path()
        .to_path_buf();

    let output_file = NamedTempFile::new()?;

    // Attempt to decrypt a non-existent file
    let result = git_remote_s3::gpg::decrypt(&nonexistent, output_file.path());
    assert!(result.is_err());

    Ok(())
}

// Helper function to get a test GPG key
fn get_test_gpg_key() -> Result<String> {
    let output = std::process::Command::new("gpg")
        .args(["--list-keys", "--with-colons"])
        .output()?;

    let output = String::from_utf8(output.stdout)?;

    // Parse the output to get the first key ID
    for line in output.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.get(0) == Some(&"pub") {
            if let Some(key_id) = fields.get(4) {
                return Ok(key_id.to_string());
            }
        }
    }

    anyhow::bail!("No GPG keys found")
}
