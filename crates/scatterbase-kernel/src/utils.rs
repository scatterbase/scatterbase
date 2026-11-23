use std::io::{self};
use std::fs::create_dir_all;
use std::path::Path;



/// Ensures that the directory for a file exists
pub fn ensure_directory_exists(file_path: &str) -> io::Result<()> {
    if let Some(parent) = Path::new(file_path).parent() {
        if !parent.exists() {
            println!("Creating directory: {}", parent.display());
            create_dir_all(parent)?;
        }
    }
    Ok(())
}
