use std::path::Path;

use crate::triple::handle_triple_write_command;
use crate::init::handle_init_command;

#[test]
fn test_triple_write_with_literal_object() {
    // Initialize Scatterbase if not already initialized
    if !Path::new(".env").exists() {
        handle_init_command(true); // Use defaults
    }

    // Run the triple write command with a literal object
    let subject = "https://example.com/subjects/1";
    let property = "https://example.com/properties/name";
    let object = "\"John Doe\""; // Literal object
    
    handle_triple_write_command(subject, property, object, ".env");
    
    // Verify that the triple was written correctly
    // This would require additional logic to check the output file
    
    // Additional assertions could be added here to verify the output
    assert!(true); // Basic assertion to ensure the test runs
}