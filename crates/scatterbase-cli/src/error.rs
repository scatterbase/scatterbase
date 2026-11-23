//! The one error a command ends with: a message for the person at the terminal.

use std::fmt;

/// Why a command did not do what it was asked. It is printed to standard error and the
/// process ends with a failure status.
#[derive(Debug)]
pub struct Error(pub String);

/// What a command returns.
pub type Result<T = ()> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self(message)
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self(message.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<scatter_claims::ClaimError> for Error {
    fn from(error: scatter_claims::ClaimError) -> Self {
        Self(error.to_string())
    }
}

/// Adds what was being done to an error: `result.context("Error reading storage")?`.
pub trait Context<T> {
    /// The error with `what` before it.
    fn context(self, what: &str) -> Result<T>;
}

impl<T, E: fmt::Display> Context<T> for std::result::Result<T, E> {
    fn context(self, what: &str) -> Result<T> {
        self.map_err(|e| Error(format!("{what}: {e}")))
    }
}
