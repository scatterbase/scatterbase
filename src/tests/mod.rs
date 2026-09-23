// Integration tests for Scatterbase
// Test data module (not a test itself, but used by tests)
pub mod test_data;

#[cfg(test)]
mod test_blank_nodes;
#[cfg(test)]
mod test_claim_prepare;
#[cfg(test)]
mod test_triple_write;
#[cfg(test)]
mod test_literal_parsing;
#[cfg(test)]
mod test_rdf_star;
#[cfg(test)]
mod test_literal;
#[cfg(test)]
mod test_hash_uris;
#[cfg(test)]
mod test_rio_literal;
#[cfg(test)]
mod test_claim_submit;
#[cfg(test)]
mod test_claim_write;
#[cfg(test)]
mod test_rdf_star_formatting;
#[cfg(test)]
mod test_prior_claim;
#[cfg(test)]
mod test_nt_storage;
#[cfg(test)]
mod test_triples_parsing;
#[cfg(test)]
mod test_dataset_parsing;
#[cfg(test)]
mod test_env_files;
#[cfg(test)]
mod test_find_latest_public_key;
