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

fn enable_ansi_support() {
    extern "system" {
        fn GetStdHandle(nStdHandle: u32) -> *mut std::ffi::c_void;
        fn GetConsoleMode(hConsoleHandle: *mut std::ffi::c_void, lpMode: *mut u32) -> i32;
        fn SetConsoleMode(hConsoleHandle: *mut std::ffi::c_void, dwMode: u32) -> i32;
    }
    const STD_OUTPUT_HANDLE: u32 = 0xFFFFFFF5u32;
    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
    unsafe {
        let handle = GetStdHandle(STD_OUTPUT_HANDLE);
        if !handle.is_null() && handle != (-1isize as *mut std::ffi::c_void) {
            let mut mode = 0u32;
            if GetConsoleMode(handle, &mut mode) != 0 {
                let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
            }
        }
    }
}

fn main() {
    enable_ansi_support();

    let cli = Cli::parse();

    if matches!(cli.command, Commands::Daemon) {
        env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

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
        env_logger::init_from_env(env_logger::Env::default().default_filter_or("warn"));

        let _instance = match ensure_single_instance() {
            Ok(inst) => inst,
            Err(_) => {
                eprintln!("Another instance of COPE is already running.");
                process::exit(1);
            }
        };

        if let Err(e) = windows::start_background(None) {
            eprintln!("COPE start error: {}", e);
            process::exit(1);
        }

        println!("COPE started.");
    } else {
        env_logger::init_from_env(env_logger::Env::default().default_filter_or("warn"));

        if let Err(e) = execute(cli) {
            error!("Error: {}", e);
            process::exit(1);
        }
    }
}
