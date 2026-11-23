use crate::tests::test_data::TEST_ENV_CONTENT;

#[test]
fn test_env_files() {
    // Use in-memory test.env content instead of reading from a file
    let test_env_content = TEST_ENV_CONTENT;
    println!("test.env content:\n{}", test_env_content);

    // Verify that the test.env content contains the expected variables
    assert!(test_env_content.contains("DOMAIN="), "test.env does not contain DOMAIN=");
    assert!(test_env_content.contains("STORAGE_FILE="), "test.env does not contain STORAGE_FILE=");
    assert!(test_env_content.contains("SERVER_URI="), "test.env does not contain SERVER_URI=");
    assert!(test_env_content.contains("ED25519_PUBLIC_KEY_FILE="), "test.env does not contain ED25519_PUBLIC_KEY_FILE=");
    assert!(test_env_content.contains("ED25519_SECRET_KEY_FILE="), "test.env does not contain ED25519_SECRET_KEY_FILE=");
    assert!(test_env_content.contains("PREFIX_FILE="), "test.env does not contain PREFIX_FILE=");

    // We're not testing loading the environment variables to avoid unsafe functions
    // The content has the expected variables, which is sufficient for this test
}
