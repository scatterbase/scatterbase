//! The `scatterbase` command-line interface: the commands, their arguments and their help.
//! Everything it does to data goes through `scatterbase-kernel`.

mod claim;
mod config;
mod error;
mod instance;
mod key;
mod log;
mod prompt;

#[cfg(test)]
mod tests;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ed25519_dalek::VerifyingKey;
use oxrdf::NamedNode;
use scatter_actors::time::Timestamp;

use crate::claim::{Source, Why};
use crate::config::Config;
use crate::error::Result;

/// A URI as a person writes it: absolute, with or without angle brackets around it.
pub fn iri(text: &str) -> std::result::Result<NamedNode, String> {
    scatterbase_kernel::uri::parse_iri(text)
}

/// Signed claims over RDF triples, kept in a verifiable log.
#[derive(Parser)]
#[command(name = "scatterbase", version, about = "Signed claims over RDF triples, kept in a verifiable log", propagate_version = true)]
struct Cli {
    /// The instance's configuration file
    #[arg(long, global = true, default_value = ".env", value_name = "PATH")]
    env: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Set up a new instance: its configuration, the server's key and its log
    Init {
        /// Ask nothing and use the defaults
        #[arg(short, long)]
        default: bool,
    },
    /// Claims about domains
    #[command(subcommand)]
    Domain(DomainCommand),
    /// Claim single triples
    #[command(subcommand)]
    Triple(TripleCommand),
    /// Make, submit and erase claims
    ///
    /// A claim is a set of triples asserted by an actor and signed with the actor's key.
    /// Every claim names who makes it with --attribution: an actor whose key 'key add'
    /// registered, or the server's own URI.
    #[command(subcommand)]
    Claim(ClaimCommand),
    /// Register, revoke, list and rotate keys
    ///
    /// Keys are used to sign claims and verify their authenticity. An actor's keys are
    /// registered in the log by the server; the server's own key is the one the log was
    /// founded on, until it is rotated.
    #[command(subcommand)]
    Key(KeyCommand),
    /// Erase actors from the log
    #[command(subcommand)]
    Actor(ActorCommand),
    /// Verify, rebuild from and export the log
    ///
    /// The log holds the server's key and graph registry in its server partition, the keys
    /// of actors in its mutable partition and every accepted claim in its claim partition.
    /// The storage file is derived from it.
    #[command(subcommand)]
    Log(LogCommand),
    /// The prefixes URIs may be written with
    #[command(subcommand)]
    Prefix(PrefixCommand),
    /// Look terms up
    #[command(subcommand)]
    Term(TermCommand),
    /// Delete everything the instance keeps: log, store, keys and configuration
    Purge {
        /// Do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum DomainCommand {
    /// Claim, as the server, that a domain is a dataset
    Init {
        /// The domain's URI [default: asked for]
        uri: Option<String>,
    },
}

#[derive(Subcommand)]
enum TripleCommand {
    /// Claim one triple, as the actor named by --attribution
    Write {
        /// The subject's URI
        #[arg(value_parser = iri)]
        subject: NamedNode,
        /// The property's URI
        #[arg(value_parser = iri)]
        property: NamedNode,
        /// The object: a URI, or a literal enclosed in double quotes
        object: String,
        /// Who makes the claim: an actor with a registered key, or the server's own URI
        #[arg(short, long, value_parser = iri)]
        attribution: NamedNode,
        /// The file of the key to sign with [default: the actor's key file]
        #[arg(short = 'k', long)]
        private_key: Option<String>,
    },
}

#[derive(Subcommand)]
enum ClaimCommand {
    /// Write the signed envelope of a claim to standard output, as Turtle
    Prepare(Source),
    /// Verify a prepared envelope and append its claim to the log
    Submit {
        /// The file of the envelope [default: standard input]
        #[arg(short, long)]
        file: Option<String>,
    },
    /// Prepare and submit a claim in one step
    Write(Source),
    /// Erase parts of an accepted claim, for good
    ///
    /// Erasing removes what was claimed (content), who claimed it (attestation) or what it
    /// acknowledged (acknowledgments). The claim keeps its ID, its place in the log and the
    /// server's signature. This cannot be undone.
    Erase {
        /// The claim's ID
        claim: String,
        /// The parts to erase: content, attestation, acknowledgments
        #[arg(long, required = true, value_delimiter = ',')]
        parts: Vec<String>,
        #[command(flatten)]
        why: Why,
    },
}

#[derive(Subcommand)]
enum KeyCommand {
    /// Register a key for an actor
    ///
    /// Without --file, a new key is generated for the actor and kept with the server's
    /// key; 'claim prepare' and 'claim write' then sign with it.
    Add {
        /// The actor's URI
        #[arg(short, long, value_parser = iri)]
        attribution: NamedNode,
        /// The file of a private key the actor already holds, whose public key to register
        #[arg(short, long)]
        file: Option<String>,
        /// When the key stops being valid, as 2027-12-31T00:00:00Z [default: never]
        #[arg(long, value_parser = key::timestamp, value_name = "TIME")]
        expires: Option<Timestamp>,
    },
    /// Revoke the keys registered for an actor, or one of them
    Revoke {
        /// The actor's URI
        #[arg(short, long, value_parser = iri)]
        attribution: NamedNode,
        /// The ID of the one key to revoke [default: all of the actor's keys]
        #[arg(long)]
        key_id: Option<String>,
    },
    /// List the server's keys, or the keys of an actor
    List {
        /// The actor's URI [default: the server]
        #[arg(short, long, value_parser = iri)]
        attribution: Option<NamedNode>,
    },
    /// Replace the server's key with a new one
    ///
    /// The log records the new key, signed by the old one. What the old key signed still
    /// verifies. The old private key is overwritten.
    Rotate,
}

#[derive(Subcommand)]
enum ActorCommand {
    /// Erase an actor's record and keys from the log, for good
    ///
    /// An actor is known in the log by a number, and one record says which URI the number
    /// stands for. Erasing the actor erases that record and the actor's key records,
    /// leaving the number. The actor then has no keys. Its claims are not touched.
    Erase {
        /// The actor's URI
        #[arg(short, long, value_parser = iri)]
        attribution: NamedNode,
        #[command(flatten)]
        why: Why,
    },
}

#[derive(Subcommand)]
enum LogCommand {
    /// Check the log: the server's key chain, the signed checkpoints, every record, every
    /// signature and that every erased part is accounted for
    Verify {
        /// Verify the bundle in this directory instead, which needs no instance
        #[arg(long, value_name = "DIRECTORY")]
        bundle: Option<String>,
        /// The server's public key to verify a bundle against, as urn:ed25519:<key>
        /// [default: the key the bundle's own chain ends in]
        #[arg(long, requires = "bundle", value_parser = log::public_key)]
        key: Option<VerifyingKey>,
    },
    /// Write the storage file again from the log
    Rebuild,
    /// Write a bundle of the log that can be verified on its own
    ///
    /// A bundle is a copy of the log's server and claim partitions that anyone can verify
    /// with nothing but the server's public key.
    Export {
        /// The directory to write the bundle to, which must not exist or must be empty
        directory: String,
        /// Also include the mutable partition, which says who the actors are. Not for
        /// publication
        #[arg(long)]
        backup: bool,
    },
}

#[derive(Subcommand)]
enum PrefixCommand {
    /// Add a prefix
    Add {
        /// The prefix, such as schema
        prefix: String,
        /// The URI it stands for, such as http://schema.org/
        uri: String,
    },
}

#[derive(Subcommand)]
enum TermCommand {
    /// Print the ID of the claim that last used a term
    Latest {
        /// The term's URI
        #[arg(value_parser = iri)]
        term: NamedNode,
    },
}

/// Runs a command.
fn run(cli: Cli) -> Result {
    let env = cli.env.as_str();
    // The two commands that need no instance
    match &cli.command {
        Command::Init { default } => return instance::init_command(env, *default),
        Command::Log(LogCommand::Verify { bundle: Some(bundle), key }) => return log::verify_bundle_command(bundle, key.as_ref()),
        _ => {},
    }

    let config = Config::load(env)?;
    match cli.command {
        Command::Init { .. } => unreachable!("handled above"),
        Command::Domain(DomainCommand::Init { uri }) => instance::domain_command(&config, uri.as_deref()),
        Command::Triple(TripleCommand::Write { subject, property, object, attribution, private_key }) => {
            claim::triple_command(&config, &subject, &property, &object, &attribution, private_key.as_deref())
        },
        Command::Claim(ClaimCommand::Prepare(source)) => claim::prepare_command(&config, &source),
        Command::Claim(ClaimCommand::Submit { file }) => claim::submit_command(&config, file.as_deref()),
        Command::Claim(ClaimCommand::Write(source)) => claim::write_command(&config, &source),
        Command::Claim(ClaimCommand::Erase { claim, parts, why }) => claim::erase_command(&config, &claim, &parts, &why),
        Command::Key(KeyCommand::Add { attribution, file, expires }) => key::add_command(&config, &attribution, file.as_deref(), expires),
        Command::Key(KeyCommand::Revoke { attribution, key_id }) => key::revoke_command(&config, &attribution, key_id.as_deref()),
        Command::Key(KeyCommand::List { attribution }) => key::list_command(&config, attribution.as_ref()),
        Command::Key(KeyCommand::Rotate) => key::rotate_command(&config),
        Command::Actor(ActorCommand::Erase { attribution, why }) => key::erase_actor_command(&config, &attribution, &why),
        Command::Log(LogCommand::Verify { .. }) => log::verify_command(&config),
        Command::Log(LogCommand::Rebuild) => log::rebuild_command(&config),
        Command::Log(LogCommand::Export { directory, backup }) => log::export_command(&config, &directory, backup),
        Command::Prefix(PrefixCommand::Add { prefix, uri }) => instance::prefix_command(&config, &prefix, &uri),
        Command::Term(TermCommand::Latest { term }) => log::latest_command(&config, &term),
        Command::Purge { yes } => instance::purge_command(&config, env, yes),
    }
}

/// When whoever reads our output goes away, as `head` does once it has its lines, end
/// quietly as other commands do. Rust ignores the signal by default, and a later print then
/// fails and panics.
#[cfg(unix)]
fn end_quietly_on_closed_pipe() {
    // SAFETY: restoring a signal's default disposition, at start-up, before any other
    // thread exists and with no handler of ours installed.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn end_quietly_on_closed_pipe() {}

fn main() -> ExitCode {
    end_quietly_on_closed_pipe();
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        },
    }
}
