//! Tests of the command line: how arguments are read, and one instance's life in a
//! temporary directory.

use clap::{CommandFactory, Parser};
use scatterbase_kernel::log::ClaimLog;
use scatterbase_kernel::lookup::Snapshot;

use crate::config::Config;
use crate::{run, Cli};

fn parse(words: &[&str]) -> Result<Cli, clap::Error> {
    Cli::try_parse_from(std::iter::once("scatterbase").chain(words.iter().copied()))
}

fn ok(words: &[&str]) {
    run(parse(words).unwrap()).unwrap_or_else(|e| panic!("{words:?}: {e}"));
}

fn fails(words: &[&str]) -> String {
    run(parse(words).unwrap()).expect_err("the command should fail").to_string()
}

#[test]
fn the_command_definitions_are_consistent() {
    Cli::command().debug_assert();
}

#[test]
fn arguments_are_checked_before_anything_runs() {
    // Every claim names who makes it
    assert!(parse(&["claim", "write", "--file", "t.nt"]).is_err());
    assert!(parse(&["triple", "write", "http://example.org/s", "http://example.org/p", "\"o\""]).is_err());
    // URIs, reasons, times and keys are read as what they are
    assert!(parse(&["claim", "write", "-a", "not a uri"]).is_err());
    assert!(parse(&["claim", "erase", "abc", "--parts", "content", "--reason", "whim", "--authority", "t:1"]).is_err());
    assert!(parse(&["claim", "erase", "abc", "--reason", "privacy", "--authority", "t:1"]).is_err());
    assert!(parse(&["key", "add", "-a", "http://example.org/a", "--expires", "next week"]).is_err());
    assert!(parse(&["log", "verify", "--bundle", "b", "--key", "urn:ed25519:short"]).is_err());
    assert!(parse(&["log", "verify", "--key", "urn:ed25519:short"]).is_err());
    assert!(parse(&["claim", "frobnicate"]).is_err());

    // The short and long forms, and the configuration file anywhere on the line
    let cli = parse(&["claim", "write", "-a", "<http://example.org/alice>", "-f", "t.nt", "--env", "other.env"]).unwrap();
    assert_eq!(cli.env, "other.env");
    assert_eq!(parse(&["--env", "other.env", "purge", "--yes"]).unwrap().env, "other.env");
    assert_eq!(parse(&["key", "list"]).unwrap().env, ".env");
    parse(&["claim", "erase", "abc", "--parts", "content,attestation", "--reason", "legal", "--authority", "t:1", "--comment", "c"]).unwrap();
}

#[test]
fn an_instance_is_set_up_used_and_taken_down() {
    // The commands write to the working directory, so work in a temporary one
    let dir = tempfile::tempdir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    let alice = "https://example.com/alice";

    // Nothing works before the instance is set up, and it is set up once
    assert!(fails(&["key", "list"]).contains("scatterbase init"));
    ok(&["init", "--default"]);
    assert!(fails(&["init", "--default"]).contains("already exists"));
    let config = Config::load(".env").unwrap();
    let claims = || ClaimLog::open(&config.instance()).unwrap().claims(0).unwrap();
    assert_eq!(claims().len(), 1);

    // An actor with no key cannot claim; with one, the triple lands in the claim graph
    let write = ["triple", "write", "https://example.com/subjects/1", "https://example.com/properties/name", "\"John Doe\"", "-a", alice];
    assert!(fails(&write).contains("key add"));
    ok(&["key", "add", "--attribution", alice]);
    assert!(fails(&["key", "add", "--attribution", alice]).contains("already registered"));
    ok(&write);
    assert_eq!(claims().len(), 2);
    let signed = claims()[1].signed().unwrap();
    assert_eq!(signed.envelope().claim().attribution().as_str(), alice);
    let claimed = Snapshot::load(&config.instance()).unwrap().triples_in(&config.instance().graph("claim"));
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].to_string(), "<https://example.com/subjects/1> <https://example.com/properties/name> \"John Doe\"");

    // The server claims as itself, by its URI
    ok(&["domain", "init", "https://example.net"]);
    ok(&["prefix", "add", "schema", "http://schema.org/"]);
    assert!(std::fs::read_to_string("data/prefixes.ttl").unwrap().contains("@prefix schema: <http://schema.org/> ."));
    ok(&["term", "latest", "https://example.com/subjects/1"]);

    // Erasure, key lifecycle, verification and a bundle
    let id = signed.id().to_string();
    assert!(fails(&["claim", "erase", &id, "--parts", "bogus", "--reason", "privacy", "--authority", "t:1"]).contains("not a part"));
    ok(&["claim", "erase", &id, "--parts", "content", "--reason", "privacy", "--authority", "t:1"]);
    assert!(claims()[1].signed().is_none());
    ok(&["key", "rotate"]);
    ok(&["key", "revoke", "-a", alice]);
    ok(&["key", "list", "-a", alice]);
    ok(&["actor", "erase", "-a", alice, "--reason", "privacy", "--authority", "t:2"]);
    ok(&["log", "verify"]);
    ok(&["log", "rebuild"]);
    ok(&["log", "export", "bundle"]);
    assert!(fails(&["log", "export", "bundle"]).contains("not empty"));
    ok(&["log", "verify", "--bundle", "bundle"]);

    // Taken down, nothing is left and it can be set up again
    ok(&["purge", "--yes"]);
    for left in [".env", "data/log", "data/scatterbase.nq", "data/prefixes.ttl", "keys/scatterbase.ed25519.private", "keys/actors"] {
        assert!(!std::path::Path::new(left).exists(), "{left} is left");
    }
    ok(&["init", "-d"]);
}
