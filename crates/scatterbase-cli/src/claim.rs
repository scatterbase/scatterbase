//! The `claim` and `triple` commands: making, submitting and erasing claims.

use std::collections::HashMap;
use std::io::{self, Cursor, Read};

use oxrdf::{Literal, NamedNode, Triple};
use scatter_claims::{ClaimId, Envelope, SignedClaim};
use scatterbase_kernel::claim::{erase_claim, parse_turtle, prepare, read_triples_from_file, read_triples_from_stdin, submit, write};
use scatterbase_kernel::log::{ClaimPart, Reason};
use scatterbase_kernel::lookup::Snapshot;
use scatterbase_kernel::prefix::read_prefixes;

use crate::config::Config;
use crate::error::{Context, Result};

/// What a claim is made from on the command line.
#[derive(clap::Args)]
pub struct Source {
    /// Who makes the claim: an actor with a registered key, or the server's own URI
    #[arg(short, long, value_parser = crate::iri)]
    pub attribution: NamedNode,
    /// The file of triples to claim, Turtle if it ends in .ttl and N-Triples otherwise [default: standard input, as Turtle]
    #[arg(short, long)]
    pub file: Option<String>,
    /// The domain blank nodes are given IRIs under [default: the instance's]
    #[arg(short, long)]
    pub domain: Option<String>,
    /// The file of the key to sign with [default: the actor's key file, which 'key add' writes]
    #[arg(short = 'k', long)]
    pub private_key: Option<String>,
}

/// Why something is erased.
#[derive(clap::Args)]
pub struct Why {
    /// The class of reason
    #[arg(long, value_parser = ["legal", "privacy", "upstream", "operational"])]
    pub reason: String,
    /// What authorizes the erasure, such as a ticket or a request
    #[arg(long)]
    pub authority: String,
    /// A public reason, kept in the record of the erasure
    #[arg(long)]
    pub comment: Option<String>,
}

impl Why {
    /// The reason as the kernel takes it.
    pub fn reason(&self) -> Reason<'_> {
        Reason { class: &self.reason, authority: &self.authority, comment: self.comment.as_deref() }
    }
}

impl Source {
    fn triples(&self, config: &Config) -> Result<Vec<Triple>> {
        let prefixes = read_prefixes(&config.prefix_file);
        let parsed = match &self.file {
            Some(file) => read_triples_from_file(file, &prefixes).context("Error reading triples from file")?,
            None => read_triples_from_stdin(&prefixes).context("Error reading triples from stdin")?,
        };
        if parsed.is_empty() {
            return Err("No triples found in input.".into());
        }
        Ok(parsed)
    }

    fn domain<'a>(&'a self, config: &'a Config) -> &'a str {
        self.domain.as_deref().unwrap_or(&config.domain)
    }
}

/// Prepares and submits a claim of triples by an actor, in one step.
pub fn write_claim(config: &Config, triples: Vec<Triple>, attribution: NamedNode, domain: &str, key_file: Option<&str>) -> Result<SignedClaim> {
    let key = config.signing_key_for(&attribution, key_file)?;
    let server_key = config.server_key()?;
    write(triples, attribution, domain, &config.instance(), &key, &server_key).context("Claim rejected")
}

/// Prints what the server assigned an accepted claim.
pub fn print_accepted(signed: &SignedClaim) {
    println!("  Claim: {}", signed.id());
    println!("  Sequence ID: {}", signed.sequence_id());
    println!("  Timestamp: {}", signed.timestamp());
    println!("  Server Signature: {}", signed.server_signature());
}

/// `claim prepare`: writes the signed envelope of a claim to standard output, as Turtle.
pub fn prepare_command(config: &Config, source: &Source) -> Result {
    let triples = source.triples(config)?;
    let key = config.signing_key_for(&source.attribution, source.private_key.as_deref())?;
    let store = Snapshot::load(&config.instance()).context("Error reading storage")?;
    let envelope = prepare(triples, source.attribution.clone(), source.domain(config), &store, &key)
        .context("Error preparing claim")?;
    print!("{}", envelope.to_turtle());
    Ok(())
}

/// `claim submit`: verifies a prepared envelope and appends its claim to the log.
pub fn submit_command(config: &Config, file: Option<&str>) -> Result {
    let text = match file {
        Some(file) => std::fs::read_to_string(file).context("Error reading file")?,
        None => {
            let mut text = String::new();
            io::stdin().read_to_string(&mut text).context("Error reading from stdin")?;
            text
        },
    };
    // An envelope is written with every IRI in full or under its own prefixes
    let triples = parse_turtle(Cursor::new(text), &HashMap::new()).context("Error parsing claim envelope")?;
    let envelope = Envelope::from_triples(&triples).context("Claim submission rejected")?;
    let signed = submit(envelope, &config.instance(), &config.server_key()?).context("Claim submission rejected")?;

    println!("Claim submitted and stored successfully.");
    print_accepted(&signed);
    Ok(())
}

/// `claim write`: prepare and submit in one step.
pub fn write_command(config: &Config, source: &Source) -> Result {
    let triples = source.triples(config)?;
    let signed = write_claim(config, triples, source.attribution.clone(), source.domain(config), source.private_key.as_deref())?;
    println!("Claim committed to data storage successfully.");
    print_accepted(&signed);
    Ok(())
}

/// `claim erase`: erases parts of an accepted claim, for good.
pub fn erase_command(config: &Config, claim: &str, parts: &[String], why: &Why) -> Result {
    let id: ClaimId = claim.parse()?;
    let names = ClaimPart::ALL.map(ClaimPart::name).join(", ");
    let parts = parts.iter()
        .map(|name| ClaimPart::named(name).ok_or_else(|| format!("`{name}` is not a part that can be erased. The parts are: {names}.")))
        .collect::<std::result::Result<Vec<ClaimPart>, String>>()?;
    let erased = erase_claim(&config.instance(), id, &parts, why.reason(), &config.server_key()?).context("Not erased")?;

    if erased.is_empty() {
        println!("Nothing to erase: claim {id} no longer has those parts.");
    } else {
        println!("Erased from claim {id}: {}", erased.iter().map(|part| part.name()).collect::<Vec<_>>().join(", "));
        println!("This cannot be undone. The claim keeps its ID, its place in the log and the server's signature.");
    }
    Ok(())
}

/// `triple write`: claims one triple, as `claim write` does a file of them.
pub fn triple_command(config: &Config, subject: &NamedNode, property: &NamedNode, object: &str, attribution: &NamedNode, key_file: Option<&str>) -> Result {
    // The object is a literal if it is enclosed in quotes, and a URI otherwise
    let triple = if object.len() >= 2 && object.starts_with('"') && object.ends_with('"') {
        Triple::new(subject.clone(), property.clone(), Literal::new_simple_literal(&object[1..object.len() - 1]))
    } else {
        Triple::new(subject.clone(), property.clone(), crate::iri(object).context("Invalid object URI")?)
    };
    let signed = write_claim(config, vec![triple], attribution.clone(), &config.domain, key_file)?;

    println!("Triple claimed successfully:");
    for triple in signed.envelope().claim().triples() {
        println!("{triple} .");
    }
    print_accepted(&signed);
    Ok(())
}
