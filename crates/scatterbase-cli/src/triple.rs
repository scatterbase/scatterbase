//! The `triple` command.

use oxrdf::{Literal, NamedNode, Triple};

use crate::claim::{print_accepted, write_claim, Options};
use crate::config::Config;

/// Handles the "triple write" command: claims one triple as the actor named by
/// --attribution, as `claim write` does for a file of them.
pub fn handle_triple_write_command(args: &[String], env_path: &str) {
    let written = Config::load(env_path).and_then(|config| {
        let (subject, property, object) = (&args[3], &args[4], &args[5]);
        let options = Options::parse(args, 6, &["--attribution", "--private-key"])?;
        let attribution = options.required_attribution()?;

        let subject = NamedNode::new(subject.as_str()).map_err(|e| format!("Invalid subject URI: {}", e))?;
        let property = NamedNode::new(property.as_str()).map_err(|e| format!("Invalid property URI: {}", e))?;
        // The object is a literal if it is enclosed in quotes, and a URI otherwise
        let triple = if object.len() >= 2 && object.starts_with('"') && object.ends_with('"') {
            Triple::new(subject, property, Literal::new_simple_literal(&object[1..object.len() - 1]))
        } else {
            let object = NamedNode::new(object.as_str()).map_err(|e| format!("Invalid object URI: {}", e))?;
            Triple::new(subject, property, object)
        };
        write_claim(&config, vec![triple], attribution, &config.domain, options.private_key.as_deref())
    });
    match written {
        Ok(signed) => {
            println!("Triple claimed successfully:");
            for triple in signed.envelope().claim().triples() {
                println!("{} .", triple);
            }
            print_accepted(&signed);
        },
        Err(e) => eprintln!("{}", e),
    }
}
