use std::io::Write;
use std::fs::File;
use tempfile::NamedTempFile;
use crate::term::find_latest_public_key_for_attribution;

#[test]
fn test_find_latest_public_key_with_problematic_triple() {
    // Create a string with the problematic triple that caused the panic
    let content = "<http://hoyyjjpfgzuytxbu.sandbox/Server> <https://scatter.red/terms/v0/hasPublicKey> <urn:ed25519:d5ajrniogsfqi4yywqxz3j3x5y3j1r6hkyktqx66to51rkup1w8o> .";

    // Create a temporary file
    let mut temp_file = NamedTempFile::new().expect("Failed to create temporary file");

    // Write the content to the file
    temp_file.write_all(content.as_bytes()).expect("Failed to write to temporary file");

    // Get the path of the temporary file
    let temp_path = temp_file.path().to_str().expect("Failed to get path as string");

    // Call the function that previously panicked
    let result = find_latest_public_key_for_attribution(temp_path, "<http://hoyyjjpfgzuytxbu.sandbox/Server>");

    // The function should now return Ok instead of panicking
    assert!(result.is_ok(), "Function should not panic");

    // If the function returns Ok(Some(key)), verify the key is correct
    if let Ok(Some(key)) = result {
        assert_eq!(key, "<urn:ed25519:d5ajrniogsfqi4yywqxz3j3x5y3j1r6hkyktqx66to51rkup1w8o>");
    }
}

#[test]
fn test_find_latest_public_key_with_edge_case() {
    // Create a string with a triple that has spaces at the end, which could cause issues
    let content = "<http://example.org/Server> <https://scatter.red/terms/v0/hasPublicKey> <urn:ed25519:abcdef>  .";

    // Create a temporary file
    let mut temp_file = NamedTempFile::new().expect("Failed to create temporary file");

    // Write the content to the file
    temp_file.write_all(content.as_bytes()).expect("Failed to write to temporary file");

    // Get the path of the temporary file
    let temp_path = temp_file.path().to_str().expect("Failed to get path as string");

    // Call the function
    let result = find_latest_public_key_for_attribution(temp_path, "<http://example.org/Server>");

    // The function should now return Ok instead of panicking
    assert!(result.is_ok(), "Function should not panic");
}

#[test]
fn test_find_latest_public_key_with_malformed_triple() {
    // Create a string with a malformed triple (missing the final period)
    let content = "<http://example.org/Server> <https://scatter.red/terms/v0/hasPublicKey> <urn:ed25519:abcdef>";

    // Create a temporary file
    let mut temp_file = NamedTempFile::new().expect("Failed to create temporary file");

    // Write the content to the file
    temp_file.write_all(content.as_bytes()).expect("Failed to write to temporary file");

    // Get the path of the temporary file
    let temp_path = temp_file.path().to_str().expect("Failed to get path as string");

    // Call the function
    let result = find_latest_public_key_for_attribution(temp_path, "<http://example.org/Server>");

    // The function should now return Ok instead of panicking
    assert!(result.is_ok(), "Function should not panic");
}

#[test]
fn test_find_latest_public_key_with_new_problematic_triple() {
    // Create a string with the problematic triple from the new issue
    let content = "<http://rrotesbdwjcqswnl.sandbox/Server> <https://scatter.red/terms/v0/hasPublicKey> <urn:ed25519:1nk4pxsibqfox5dob8ksrkpto9cseqfss4hg6fx5amdokmmm79mo> .";

    // Create a temporary file
    let mut temp_file = NamedTempFile::new().expect("Failed to create temporary file");

    // Write the content to the file
    temp_file.write_all(content.as_bytes()).expect("Failed to write to temporary file");

    // Get the path of the temporary file
    let temp_path = temp_file.path().to_str().expect("Failed to get path as string");

    // Call the function that previously had issues
    let result = find_latest_public_key_for_attribution(temp_path, "<http://rrotesbdwjcqswnl.sandbox/Server>");

    // The function should now return Ok instead of panicking
    assert!(result.is_ok(), "Function should not panic");

    // If the function returns Ok(Some(key)), verify the key is correct
    if let Ok(Some(key)) = result {
        assert_eq!(key, "<urn:ed25519:1nk4pxsibqfox5dob8ksrkpto9cseqfss4hg6fx5amdokmmm79mo>");
    }
}
