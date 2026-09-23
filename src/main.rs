use std::env;
use std::fs;
use dotenv::dotenv;

use scatterbase::cli::{
    print_help, print_init_help, print_domain_help, print_triple_help,
    print_claim_help, print_prefix_help, print_purge_help, get_env_path,
    prompt_for_domain, prompt_for_storage_file, prompt_for_env_file,
    prompt_for_server_uri
};
use scatterbase::key::{handle_key_add_command, print_key_help};
use scatterbase::term::{handle_term_latest_command, print_term_help};
use scatterbase::domain::{handle_domain_init_command, create_env_file, create_initial_triple, create_initial_claim};
use scatterbase::utils::{
    ensure_directory_exists
};
use scatterbase::constants::DEFAULT_PREFIX_FILE;
use scatterbase::claim::{handle_claim_prepare_command, handle_claim_submit_command, handle_claim_write_command};
use scatterbase::prefix::{handle_prefix_add_command, init_prefix_file};
use scatterbase::triple::{Triple, handle_triple_write_command};
use scatterbase::uri::Uri;
use scatterbase::literal::Literal;
use scatterbase::storage::{Storage, NtFileStorage};
use scatterbase::init::handle_init_command;
use scatterbase::purge::handle_purge_command;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        match args[1].as_str() {
            "help" => {
                if args.len() > 2 {
                    // Handle "scatterbase help <aspect>"
                    match args[2].as_str() {
                        "init" => print_init_help(),
                        "domain" => print_domain_help(),
                        "triple" => print_triple_help(),
                        "claim" => print_claim_help(),
                        "key" => print_key_help(),
                        "prefix" => print_prefix_help(),
                        "purge" => print_purge_help(),
                        "term" => print_term_help(),
                        _ => {
                            println!("Unknown command: {}. Use 'scatterbase help' for a list of available commands.", args[2]);
                        }
                    }
                } else {
                    // Just "scatterbase help"
                    print_help();
                }
            },
            "init" => {
                if args.len() > 2 && (args[2] == "help" || args[2] == "--help") {
                    print_init_help();
                } else {
                    // Check if -d or --default flag is present
                    let use_defaults = args.len() > 2 && (args[2] == "-d" || args[2] == "--default");
                    handle_init_command(use_defaults);
                }
            },
            "triple" => {
                if args.len() > 2 {
                    if args[2] == "help" || args[2] == "--help" {
                        print_triple_help();
                    } else if args[2] == "write" && args.len() >= 6 {
                        let env_path = get_env_path(&args);
                        handle_triple_write_command(&args[3], &args[4], &args[5], &env_path);
                    } else {
                        println!("Invalid triple command. Use 'scatterbase help triple' for usage information.");
                    }
                } else {
                    print_triple_help();
                }
            },
            "claim" => {
                if args.len() > 2 {
                    if args[2] == "help" || args[2] == "--help" {
                        print_claim_help();
                    } else if args[2] == "prepare" {
                        let env_path = get_env_path(&args);
                        handle_claim_prepare_command(&args, &env_path);
                    } else if args[2] == "submit" {
                        let env_path = get_env_path(&args);
                        handle_claim_submit_command(&args, &env_path);
                    } else if args[2] == "write" {
                        let env_path = get_env_path(&args);
                        handle_claim_write_command(&args, &env_path);
                    } else {
                        println!("Invalid claim command. Use 'scatterbase help claim' for usage information.");
                    }
                } else {
                    print_claim_help();
                }
            },
            "domain" => {
                if args.len() > 2 {
                    if args[2] == "help" || args[2] == "--help" {
                        print_domain_help();
                    } else if args[2] == "init" {
                        let env_path = get_env_path(&args);
                        // Check if a URI was provided as an argument
                        let uri_param = if args.len() > 3 { Some(args[3].as_str()) } else { None };
                        handle_domain_init_command(&env_path, uri_param);
                    } else {
                        println!("Invalid domain command. Use 'scatterbase help domain' for usage information.");
                    }
                } else {
                    print_domain_help();
                }
            },
            "prefix" => {
                if args.len() > 2 {
                    if args[2] == "help" || args[2] == "--help" {
                        print_prefix_help();
                    } else if args[2] == "add" && args.len() >= 5 {
                        handle_prefix_add_command(&args[3], &args[4]);
                    } else {
                        println!("Invalid prefix command. Use 'scatterbase help prefix' for usage information.");
                    }
                } else {
                    print_prefix_help();
                }
            },
            "purge" => {
                if args.len() > 2 && (args[2] == "help" || args[2] == "--help") {
                    print_purge_help();
                } else {
                    let env_path = get_env_path(&args);
                    handle_purge_command(&env_path);
                }
            },
            "key" => {
                if args.len() > 2 {
                    if args[2] == "help" || args[2] == "--help" {
                        print_key_help();
                    } else if args[2] == "add" {
                        let env_path = get_env_path(&args);
                        if let Err(e) = handle_key_add_command(&args, &env_path) {
                            eprintln!("Error: {}", e);
                        }
                    } else {
                        println!("Invalid key command. Use 'scatterbase help key' for usage information.");
                    }
                } else {
                    print_key_help();
                }
            },
            "term" => {
                if args.len() > 2 {
                    if args[2] == "help" || args[2] == "--help" {
                        print_term_help();
                    } else if args[2] == "latest" && args.len() > 3 {
                        let env_path = get_env_path(&args);
                        if let Err(e) = handle_term_latest_command(&args[3], &env_path) {
                            eprintln!("Error: {}", e);
                        }
                    } else {
                        println!("Invalid term command. Use 'scatterbase help term' for usage information.");
                    }
                } else {
                    print_term_help();
                }
            },
            _ => {
                println!("Unknown command: {}. Use 'scatterbase help' for a list of available commands.", args[1]);
            }
        }
    } else {
        // No command provided, show help
        print_help();
    }
}
