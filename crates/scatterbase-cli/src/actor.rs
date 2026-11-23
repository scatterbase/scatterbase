//! The `actor` command.

use scatterbase_kernel::claim::erase_actor;

use crate::claim::Options;
use crate::config::Config;

/// Handles the "actor erase" command: erases an actor's record and keys from the log, for
/// good. The claims the actor made are not touched.
pub fn handle_actor_erase_command(args: &[String], env_path: &str) {
    let erased = Config::load(env_path).and_then(|config| {
        let options = Options::parse(args, 3, &["--attribution", "--reason", "--authority", "--comment"])?;
        let actor = options.attribution.clone()
            .ok_or("Attribution URI is required. Use --attribution to name the actor.")?;
        let server_key = config.server_key().map_err(|e| e.to_string())?;
        let surrogate = erase_actor(&config.instance(), &actor, options.reason()?, &server_key)
            .map_err(|e| format!("Not erased: {}", e))?;
        Ok((actor, surrogate))
    });
    match erased {
        Ok((actor, None)) => println!("Nothing to erase: the log has no record of {}.", actor.as_str()),
        Ok((actor, Some(surrogate))) => {
            println!("Erased {} from the log: its record and its keys, which were under actor:{}.", actor.as_str(), surrogate);
            println!("This cannot be undone. The claims it made still name it; erase their attestations with 'scatterbase claim erase'.");
        },
        Err(e) => println!("{}", e),
    }
}

/// Prints help information for the actor command
pub fn print_actor_help() {
    crate::cli::print_help_info(
        "actor",
        "scatterbase actor <command> [options]",
        Some(vec![
            ("erase", "Erase an actor's record and keys from the log, for good"),
        ]),
        Some(vec![
            "An actor is known in the log by a number, and one record says which URI the number",
            "stands for. Erasing the actor erases that record and the actor's key records, leaving",
            "the number. The actor then has no keys. Its claims are not touched."
        ]),
        Some(vec![
            "scatterbase actor erase --attribution https://example.org/alice --reason privacy --authority ticket:4411",
        ]),
        true
    );
}
