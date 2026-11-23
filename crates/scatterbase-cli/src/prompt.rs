//! Asking the person at the terminal.

use std::io::{self, Write};

use crate::error::Result;

/// Asks a question and returns the answer, or the default if the answer is empty.
pub fn ask(question: &str, default: &str) -> Result<String> {
    print!("{question} [{default}]: ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    let answer = answer.trim();
    Ok(if answer.is_empty() { default } else { answer }.to_string())
}

/// Asks for a `yes` and returns whether it was given.
pub fn confirm(warning: &str) -> Result<bool> {
    println!("{warning}");
    println!("Type 'yes' to confirm or anything else to cancel.");
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(answer.trim() == "yes")
}
