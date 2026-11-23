// Integration tests for Scatterbase
// Test data module (not a test itself, but used by tests)
pub mod test_data;

#[cfg(test)]
mod test_literal_parsing;
#[cfg(test)]
mod test_literal;
#[cfg(test)]
mod test_triples_parsing;
#[cfg(test)]
mod test_dataset_parsing;
