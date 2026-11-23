//! The `claim` commands: prepare, submit, write and erase.

use std::collections::HashMap;
use std::io::{self, Cursor, Read};

use oxrdf::{NamedNode, Triple};
use scatter_claims::{ClaimId, Envelope, SignedClaim};
use scatterbase_kernel::claim::{erase_claim, parse_turtle, prepare, read_triples_from_file, read_triples_from_stdin, submit};
use scatterbase_kernel::log::{ClaimPart, Reason, REASON_CLASSES};
use scatterbase_kernel::lookup::Snapshot;
use scatterbase_kernel::prefix::read_prefixes;
use scatterbase_kernel::uri::parse_iri;

use crate::config::Config;

/// The options of the commands that make or erase claims.
#[derive(Default)]
pub struct Options {
    pub attribution: Option<NamedNode>,
    pub file: Option<String>,
    pub domain: Option<String>,
    pub private_key: Option<String>,
    pub parts: Option<String>,
    pub reason: Option<String>,
    pub authority: Option<String>,
    pub comment: Option<String>,
}

impl Options {
    /// Parses the options from `args[start]` on, of which `allowed` are accepted.
    pub fn parse(args: &[String], start: usize, allowed: &[&str]) -> Result<Self, String> {
        let mut options = Self::default();
        let mut i = start;
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
                "--parts" => options.parts = Some(value.clone()),
                "--reason" => options.reason = Some(value.clone()),
                "--authority" => options.authority = Some(value.clone()),
                "--comment" => options.comment = Some(value.clone()),
                // --env was read before the command was dispatched
                _ => {},
            }
            i += 2;
        }
        Ok(options)
    }

    /// The actor a claim is made as, which every claim must name.
    pub fn required_attribution(&self) -> Result<NamedNode, String> {
        self.attribution.clone().ok_or_else(|| "Attribution URI is required. Use --attribution to say who makes the claim.".to_string())
    }

    /// The reason of an erasure, which must give its class and its authority.
    pub fn reason(&self) -> Result<Reason<'_>, String> {
        Ok(Reason {
            class: self.reason.as_deref()
                .ok_or_else(|| format!("A reason is required. Use --reason with one of: {}.", REASON_CLASSES.join(", ")))?,
            authority: self.authority.as_deref()
                .ok_or("An authority is required. Use --authority to say what authorizes the erasure, such as a ticket or a request.")?,
            comment: self.comment.as_deref(),
        })
    }
}

/// Prepares a claim of parsed triples by an actor, signed with the actor's key.
pub fn prepare_claim(config: &Config, parsed: Vec<Triple>, attribution: NamedNode, domain: &str, key_file: Option<&str>) -> Result<Envelope, String> {
    if parsed.is_empty() {
        return Err("No triples found in input.".to_string());
    }
    let key = config.signing_key_for(&attribution, key_file).map_err(|e| e.to_string())?;
    let store = Snapshot::load(&config.instance()).map_err(|e| format!("Error reading storage: {}", e))?;
    prepare(parsed, attribution, domain, &store, &key).map_err(|e| format!("Error preparing claim: {}", e))
}

/// Prepares and submits a claim of parsed triples by an actor, as `claim write` does.
pub fn write_claim(config: &Config, parsed: Vec<Triple>, attribution: NamedNode, key_file: Option<&str>) -> Result<SignedClaim, String> {
    let envelope = prepare_claim(config, parsed, attribution, &config.domain, key_file)?;
    let server_key = config.server_key().map_err(|e| e.to_string())?;
    submit(envelope, &config.instance(), &server_key).map_err(|e| format!("Claim rejected: {}", e))
}

/// Reads the input triples and prepares the envelope, as `claim prepare` and `claim write`
/// both do.
fn prepare_from_input(args: &[String], config: &Config) -> Result<Envelope, String> {
    let options = Options::parse(args, 3, &["--attribution", "--file", "--domain", "--private-key"])?;
    let attribution = options.required_attribution()?;
    let domain = options.domain.clone().unwrap_or_else(|| config.domain.clone());

    let prefixes = read_prefixes();
    let parsed = match &options.file {
        Some(file) => read_triples_from_file(file, &prefixes)
            .map_err(|e| format!("Error reading triples from file: {}", e))?,
        None => read_triples_from_stdin(&prefixes)
            .map_err(|e| format!("Error reading triples from stdin: {}", e))?,
    };
    prepare_claim(config, parsed, attribution, &domain, options.private_key.as_deref())
}

pub fn print_accepted(signed: &SignedClaim) {
    println!("  Claim: {}", signed.id());
    println!("  Sequence ID: {}", signed.sequence_id());
    println!("  Timestamp: {}", signed.timestamp());
    println!("  Server Signature: {}", signed.server_signature());
}

/// Handles the "claim prepare" command
/// This command accepts a filename or stdin with NT or TTL triples,
/// and returns all the triples of the ClaimEnvelope in TTL form to stdout.
/// The claim is made as the actor named by --attribution.
pub fn handle_claim_prepare_command(args: &[String], env_path: &str) {
    let prepared = Config::load(env_path).and_then(|config| prepare_from_input(args, &config));
    match prepared {
        Ok(envelope) => print!("{}", envelope.to_turtle()),
        Err(e) => eprintln!("{}", e),
    }
}

/// Handles the "claim submit" command
/// This command takes a file with a prepared claim envelope as a parameter or through stdin,
/// completes the process to make a full Signed Claim, and then saves everything to data storage.
pub fn handle_claim_submit_command(args: &[String], env_path: &str) {
    let submitted = Config::load(env_path).and_then(|config| {
        let options = Options::parse(args, 3, &["--file"])?;
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
/// The claim is made as the actor named by --attribution.
pub fn handle_claim_write_command(args: &[String], env_path: &str) {
    let written = Config::load(env_path).and_then(|config| {
        let envelope = prepare_from_input(args, &config)?;
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

/// Handles the "claim erase" command: erases parts of an accepted claim, for good.
pub fn handle_claim_erase_command(args: &[String], env_path: &str) {
    let erased = Config::load(env_path).and_then(|config| {
        let id: ClaimId = args.get(3)
            .ok_or("A claim ID is required: scatterbase claim erase <claim> --parts <parts> --reason <class> --authority <text>")?
            .parse().map_err(|e| format!("{}", e))?;
        let options = Options::parse(args, 4, &["--parts", "--reason", "--authority", "--comment"])?;
        let names = ClaimPart::ALL.map(ClaimPart::name).join(", ");
        let parts = options.parts.as_deref()
            .ok_or_else(|| format!("The parts to erase are required. Use --parts with any of: {}.", names))?
            .split(',')
            .map(|name| ClaimPart::named(name.trim()).ok_or_else(|| format!("`{}` is not a part that can be erased. The parts are: {}.", name.trim(), names)))
            .collect::<Result<Vec<ClaimPart>, String>>()?;
        let server_key = config.server_key().map_err(|e| e.to_string())?;
        let erased = erase_claim(&config.instance(), id, &parts, options.reason()?, &server_key)
            .map_err(|e| format!("Not erased: {}", e))?;
        Ok((id, erased))
    });
    match erased {
        Ok((id, erased)) if erased.is_empty() => println!("Nothing to erase: claim {} no longer has those parts.", id),
        Ok((id, erased)) => {
            println!("Erased from claim {}: {}", id, erased.iter().map(|part| part.name()).collect::<Vec<_>>().join(", "));
            println!("This cannot be undone. The claim keeps its ID, its place in the log and the server's signature.");
        },
        Err(e) => println!("{}", e),
    }
}
