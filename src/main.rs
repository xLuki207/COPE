mod cli;
mod config;
mod hotkeys;
mod parser;
mod routes;
mod windows;

use cli::{execute, Cli, Commands};
use config::Config;
use log::error;
use std::process;
use windows::ensure_single_instance;

fn main() {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let cli = Cli::parse();

    if matches!(cli.command, Commands::Daemon) {
        // Internal command: run daemon loop in current process
        let _instance = match ensure_single_instance() {
            Ok(inst) => inst,
            Err(_) => {
                process::exit(1);
            }
        };

        let config = Config::load().unwrap_or_default();
        if let Err(e) = windows::run_daemon(config) {
            error!("Daemon error: {}", e);
            process::exit(1);
        }
    } else if matches!(cli.command, Commands::Help) {
        println!("{}", Cli::help());
        process::exit(0);
    } else if matches!(cli.command, Commands::Start) {
        // Start daemon silently in background
        let _instance = match ensure_single_instance() {
            Ok(inst) => inst,
            Err(_) => {
                eprintln!("Another instance of COPE is already running.");
                process::exit(1);
            }
        };

        if let Err(e) = windows::start_background() {
            eprintln!("COPE start error: {}", e);
            process::exit(1);
        }

        println!("COPE started.");
    } else {
        if let Err(e) = execute(cli) {
            error!("Error: {}", e);
            process::exit(1);
        }
    }
}
