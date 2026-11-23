//! The `claim` commands: prepare, submit and write.

use std::collections::HashMap;
use std::io::{self, Cursor, Read};

use oxrdf::NamedNode;
use scatter_claims::{Envelope, SignedClaim};
use scatterbase_kernel::claim::{parse_turtle, prepare, read_triples_from_file, read_triples_from_stdin, submit};
use scatterbase_kernel::lookup::Snapshot;
use scatterbase_kernel::prefix::read_prefixes;
use scatterbase_kernel::uri::parse_iri;

use crate::config::Config;

/// The options of `claim prepare` and `claim write`.
#[derive(Default)]
struct Options {
    attribution: Option<NamedNode>,
    file: Option<String>,
    domain: Option<String>,
    private_key: Option<String>,
}

impl Options {
    /// Parses the options after `claim <command>`, of which `allowed` are accepted.
    fn parse(args: &[String], allowed: &[&str]) -> Result<Self, String> {
        let mut options = Self::default();
        let mut i = 3;
        while i < args.len() {
            let name = match args[i].as_str() {
                "-a" => "--attribution",
                "-f" => "--file",
                "-d" => "--domain",
                "-k" => "--private-key",
                other => other,
            };
            if name != "--env" && !allowed.contains(&name) {
                return Err(format!("Unknown option: {}", args[i]));
            }
            let value = args.get(i + 1).ok_or_else(|| format!("Missing value for {}", name))?;
            match name {
                "--attribution" => {
                    options.attribution = Some(parse_iri(value)
                        .map_err(|e| format!("Invalid value for --attribution: {}", e))?);
                },
                "--file" => options.file = Some(value.clone()),
                "--domain" => options.domain = Some(value.clone()),
                "--private-key" => options.private_key = Some(value.clone()),
                // --env was read before the command was dispatched
                _ => {},
            }
            i += 2;
        }
        Ok(options)
    }
}

/// Reads the input triples and prepares the envelope, as `claim prepare` and `claim write`
/// both do.
fn prepare_from_input(args: &[String], config: &Config) -> Result<(Envelope, String), String> {
    let options = Options::parse(args, &["--attribution", "--file", "--domain", "--private-key"])?;
    let domain = options.domain.unwrap_or_else(|| config.domain.clone());
    // If no attribution is given, the server claims
    let attribution = options.attribution.unwrap_or_else(|| config.server.clone());

    let prefixes = read_prefixes();
    let parsed = match &options.file {
        Some(file) => read_triples_from_file(file, &prefixes)
            .map_err(|e| format!("Error reading triples from file: {}", e))?,
        None => read_triples_from_stdin(&prefixes)
            .map_err(|e| format!("Error reading triples from stdin: {}", e))?,
    };
    if parsed.is_empty() {
        return Err("No triples found in input.".to_string());
    }

    let key = config.signing_key_for(&attribution, options.private_key.as_deref()).map_err(|e| e.to_string())?;
    let store = Snapshot::load(&config.instance()).map_err(|e| format!("Error reading storage: {}", e))?;
    let envelope = prepare(parsed, attribution, &domain, &store, &key)
        .map_err(|e| format!("Error preparing claim: {}", e))?;
    Ok((envelope, domain))
}

fn print_accepted(signed: &SignedClaim) {
    println!("  Claim: {}", signed.id());
    println!("  Sequence ID: {}", signed.sequence_id());
    println!("  Timestamp: {}", signed.timestamp());
    println!("  Server Signature: {}", signed.server_signature());
}

/// Handles the "claim prepare" command
/// This command accepts a filename or stdin with NT or TTL triples,
/// and returns all the triples of the ClaimEnvelope in TTL form to stdout.
/// If an attribution URI is not specified, the server claims.
pub fn handle_claim_prepare_command(args: &[String], env_path: &str) {
    let prepared = Config::load(env_path).and_then(|config| prepare_from_input(args, &config));
    match prepared {
        Ok((envelope, _)) => print!("{}", envelope.to_turtle()),
        Err(e) => eprintln!("{}", e),
    }
}

/// Handles the "claim submit" command
/// This command takes a file with a prepared claim envelope as a parameter or through stdin,
/// completes the process to make a full Signed Claim, and then saves everything to data storage.
pub fn handle_claim_submit_command(args: &[String], env_path: &str) {
    let submitted = Config::load(env_path).and_then(|config| {
        let options = Options::parse(args, &["--file"])?;
        let text = match &options.file {
            Some(file) => std::fs::read_to_string(file).map_err(|e| format!("Error reading file: {}", e))?,
            None => {
                let mut text = String::new();
                io::stdin().read_to_string(&mut text).map_err(|e| format!("Error reading from stdin: {}", e))?;
                text
            },
        };
        // An envelope is written with every IRI in full or under its own prefixes
        let triples = parse_turtle(Cursor::new(text), &HashMap::new())
            .map_err(|e| format!("Error parsing claim envelope: {}", e))?;
        let envelope = Envelope::from_triples(&triples)
            .map_err(|e| format!("Claim submission rejected: {}", e))?;

        let server_key = config.server_key().map_err(|e| e.to_string())?;
        submit(envelope, &config.instance(), &server_key)
            .map_err(|e| format!("Claim submission rejected: {}", e))
    });
    match submitted {
        Ok(signed) => {
            println!("Claim submitted and stored successfully.");
            print_accepted(&signed);
        },
        Err(e) => println!("{}", e),
    }
}

/// Handles the "claim write" command: prepare and submit in one step.
/// If an attribution URI is not specified, the server claims.
pub fn handle_claim_write_command(args: &[String], env_path: &str) {
    let written = Config::load(env_path).and_then(|config| {
        let (envelope, _) = prepare_from_input(args, &config)?;
        let server_key = config.server_key().map_err(|e| e.to_string())?;
        submit(envelope, &config.instance(), &server_key)
            .map_err(|e| format!("Claim rejected: {}", e))
    });
    match written {
        Ok(signed) => {
            println!("Claim committed to data storage successfully.");
            print_accepted(&signed);
        },
        Err(e) => println!("{}", e),
    }
}
