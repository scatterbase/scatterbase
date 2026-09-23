use std::env;
use std::io::{self, Write};
use crate::storage::generate_default_domain;
use crate::uri::Uri;
use crate::triple::Triple;
use crate::constants::*;

// Constants are now in the constants module

/// Gets the environment file path from command line arguments
pub fn get_env_path(args: &[String]) -> String {
    let mut env_path = DEFAULT_ENV_FILE.to_string();
    let mut i = 0;

    while i < args.len() {
        if args[i] == "--env" && i + 1 < args.len() {
            env_path = args[i + 1].clone();
            break;
        }
        i += 1;
    }

    env_path
}

/// Prints program information
pub fn print_program_info() {
    println!("{} - A decentralized triple store", PROGRAM_NAME);
    println!("Version: {}", env!("CARGO_PKG_VERSION"));
    println!("Author: {}", env!("CARGO_PKG_AUTHORS"));
    println!();
    println!("Usage: scatterbase <command> [options]");
    println!();
    println!("For help on a specific command, use:");
    println!("  scatterbase help <command>");
    println!();
}

/// Prints help information for a specific aspect
pub fn print_help_info(
    aspect: &str,
    usage: &str,
    commands: Option<Vec<(&str, &str)>>,
    description: Option<Vec<&str>>,
    examples: Option<Vec<&str>>,
    show_env_option: bool
) {
    println!("Help for: {}", aspect);
    println!();

    if let Some(desc_lines) = description {
        for line in desc_lines {
            println!("{}", line);
        }
        println!();
    }

    println!("Usage: {}", usage);
    println!();

    if let Some(cmds) = commands {
        println!("Commands:");
        for (cmd, desc) in cmds {
            println!("  {:<15} {}", cmd, desc);
        }
        println!();
    }

    if show_env_option {
        println!("Options:");
        println!("{}", ENV_OPTION_DESC);
        println!();
    }

    if let Some(ex) = examples {
        println!("Examples:");
        for example in ex {
            println!("  {}", example);
        }
        println!();
    }
}

/// Prints general help information
pub fn print_help() {
    print_program_info();

    let commands = vec![
        ("init", "Initialize a new Scatterbase instance"),
        ("domain", "Domain management commands"),
        ("triple", "Triple management commands"),
        ("claim", "Claim management commands"),
        ("key", "Key management commands"),
        ("prefix", "Prefix management commands"),
        ("purge", "Purge data commands"),
        ("term", "Term-related commands"),
        ("help", "Show help information"),
    ];

    print_help_info(
        "scatterbase",
        "scatterbase <command> [options]",
        Some(commands),
        None,
        None,
        false
    );
}

/// Prints help information for the init command
pub fn print_init_help() {
    let description = vec![
        "Initializes a new Scatterbase instance by creating a .env file",
        "with configuration settings and generating cryptographic keys.",
        "",
        "Options:",
        "  -d, --default    Use default values for all prompts without asking for input"
    ];

    let examples = vec![
        "scatterbase init",
        "scatterbase init -d",
        "scatterbase init --default"
    ];

    print_help_info(
        "init",
        "scatterbase init [options]",
        None,
        Some(description),
        Some(examples),
        false
    );
}

/// Prints help information for the domain command
pub fn print_domain_help() {
    let description = vec![
        "Manages domains in Scatterbase."
    ];

    let commands = vec![
        ("init", "Initialize a domain with a default triple"),
    ];

    let examples = vec![
        "scatterbase domain init",
        "scatterbase domain init --env custom.env"
    ];

    print_help_info(
        "domain",
        "scatterbase domain <command> [options]",
        Some(commands),
        Some(description),
        Some(examples),
        true
    );
}

/// Prints help information for the triple command
pub fn print_triple_help() {
    let description = vec![
        "Manages triples in Scatterbase."
    ];

    let commands = vec![
        ("write", "Write a triple to the store"),
    ];

    let examples = vec![
        "scatterbase triple write <subject> <property> <object>",
        "scatterbase triple write https://example.org http://schema.org/name \"Example\"",
        "scatterbase triple write --env custom.env <subject> <property> <object>"
    ];

    print_help_info(
        "triple",
        "scatterbase triple <command> <subject> <property> <object> [options]",
        Some(commands),
        Some(description),
        Some(examples),
        true
    );
}

/// Prints help information for the claim command
pub fn print_claim_help() {
    let description = vec![
        "Manages claims in Scatterbase.",
        "Claims are signed statements about triples."
    ];

    let commands = vec![
        ("prepare", "Prepare a claim envelope from triples"),
        ("submit", "Submit a prepared claim envelope to data storage"),
    ];

    let examples = vec![
        "scatterbase claim prepare --file triples.nt > claim_envelope.ttl",
        "scatterbase claim prepare --attribution alice --file triples.nt > claim_envelope.ttl",
        "cat triples.nt | scatterbase claim prepare > claim_envelope.ttl",
        "scatterbase claim submit --file claim_envelope.ttl",
        "cat claim_envelope.ttl | scatterbase claim submit"
    ];

    print_help_info(
        "claim",
        "scatterbase claim <command> [options]",
        Some(commands),
        Some(description),
        Some(examples),
        true
    );
}

/// Prints help information for the prefix command
pub fn print_prefix_help() {
    let description = vec![
        "Manages URI prefixes in Scatterbase.",
        "Prefixes are shortcuts for URI namespaces."
    ];

    let commands = vec![
        ("add", "Add a new prefix"),
    ];

    let examples = vec![
        "scatterbase prefix add schema http://schema.org/",
        "scatterbase prefix add foaf http://xmlns.com/foaf/0.1/"
    ];

    print_help_info(
        "prefix",
        "scatterbase prefix <command> <prefix> <uri>",
        Some(commands),
        Some(description),
        Some(examples),
        false
    );
}

/// Prints help information for the purge command
pub fn print_purge_help() {
    let description = vec![
        "Purges data from Scatterbase.",
        "WARNING: This will delete all data!"
    ];

    let examples = vec![
        "scatterbase purge",
        "scatterbase purge --env custom.env"
    ];

    print_help_info(
        "purge",
        "scatterbase purge [options]",
        None,
        Some(description),
        Some(examples),
        true
    );
}

/// Prints information about a URI
pub fn print_uri_info(uri: &Uri) {
    println!("URI Information:");
    println!("  Full URI: {}", uri.to_string());

    if let Some(scheme) = uri.scheme() {
        println!("  Scheme: {}", scheme);
    }

    if let Some(authority) = uri.authority() {
        println!("  Authority: {}", authority);
    }

    if let Some(path) = uri.path() {
        println!("  Path: {}", path);
    }

    if let Some(query) = uri.query() {
        println!("  Query: {}", query);
    }

    if let Some(fragment) = uri.fragment() {
        println!("  Fragment: {}", fragment);
    }
}

/// Prints information about a triple
pub fn print_triple_info(triple: &Triple) {
    println!("Triple Information:");
    println!("  Subject: {}", triple.subject().to_string());
    println!("  Property: {}", triple.property().to_string());
    println!("  Object: {}", triple.object().to_string());
}

/// Prompts for a domain name
pub fn prompt_for_domain() -> String {
    let default_domain = generate_default_domain();
    prompt_user(
        "Enter domain name (e.g., example.org)",
        Some(&default_domain),
        true
    )
}

/// Prompts for a storage file path
pub fn prompt_for_storage_file() -> String {
    prompt_user("Enter storage file path", Some(DEFAULT_STORAGE_FILE), true)
}

/// Prompts for an environment file path
pub fn prompt_for_env_file() -> String {
    prompt_user("Enter .env file path", Some(DEFAULT_ENV_FILE), true)
}

/// Prompts for a server URI
pub fn prompt_for_server_uri(domain: &str) -> String {
    let default_uri = format!("{}/Server", domain);
    prompt_user("Enter server URI", Some(&default_uri), true)
}

/// Prompts for a prefix file path
pub fn prompt_for_prefix_file() -> String {
    prompt_user("Enter prefix file path", Some(DEFAULT_PREFIX_FILE), true)
}

/// Prompts for a key file path
pub fn prompt_for_key_file() -> String {
    prompt_user("Enter private key file path", Some(DEFAULT_PRIVATE_KEY_FILE), true)
}

/// Prompts the user for input with an optional default value
pub fn prompt_user(prompt: &str, default: Option<&str>, show_default: bool) -> String {
    let mut input = String::new();
    let prompt_text = if let Some(default_value) = default {
        if show_default {
            format!("{} [{}]: ", prompt, default_value)
        } else {
            format!("{}: ", prompt)
        }
    } else {
        format!("{}: ", prompt)
    };

    print!("{}", prompt_text);
    io::stdout().flush().unwrap();
    io::stdin().read_line(&mut input).unwrap();

    let input = input.trim();
    if input.is_empty() {
        if let Some(default_value) = default {
            default_value.to_string()
        } else {
            String::new()
        }
    } else {
        input.to_string()
    }
}
