use scatterbase_kernel::log::ClaimLog;
use scatterbase_kernel::lookup::Snapshot;

use crate::config::Config;
use crate::init::handle_init_command;
use crate::key::handle_key_add_command;
use crate::triple::handle_triple_write_command;

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| word.to_string()).collect()
}

#[test]
fn test_triple_write_claims_the_triple() {
    // The commands write to the working directory, so work in a temporary one
    let dir = tempfile::tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    handle_init_command(true); // Use defaults
    let config = Config::load(".env").unwrap();
    let alice = "https://example.com/alice";

    // Without an attribution nothing is claimed
    let write = ["scatterbase", "triple", "write", "https://example.com/subjects/1", "https://example.com/properties/name", "\"John Doe\""];
    handle_triple_write_command(&args(&write), ".env");
    assert_eq!(ClaimLog::open(&config.instance()).unwrap().claims(0).unwrap().len(), 1);

    // As an actor with a key, the triple is claimed and lands in the claim graph
    handle_key_add_command(&args(&["scatterbase", "key", "add", "--attribution", alice]), ".env").unwrap();
    let mut attributed = write.to_vec();
    attributed.extend(["--attribution", alice]);
    handle_triple_write_command(&args(&attributed), ".env");

    let claims = ClaimLog::open(&config.instance()).unwrap().claims(0).unwrap();
    assert_eq!(claims.len(), 2);
    let signed = claims[1].signed().unwrap();
    assert_eq!(signed.envelope().claim().attribution().as_str(), alice);
    let store = Snapshot::load(&config.instance()).unwrap();
    let claimed = store.triples_in(&config.instance().graph("claim"));
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].to_string(), "<https://example.com/subjects/1> <https://example.com/properties/name> \"John Doe\"");
}
