use crate::config::{config_dir, Config};
use crate::routes::Destination;
use crate::windows::{
    disable_startup, enable_startup, is_daemon_running, is_startup_enabled, remove_from_user_path,
    start_background, stop_daemon,
};
use anyhow::Result;
use std::io::{self, Write};
use std::path::PathBuf;

const COPE_WORDMARK: &str = "COPE";
const COPE_TAGLINE: &str = "route any CA. instantly.";
const COPE_MEMECOIN_TAGLINE: &str = "built for the Solana trenches.";

// Manual CLI enum - no clap Subcommand derive to avoid conflicts
#[derive(Clone, Debug)]
pub enum Commands {
    /// Install and start COPE
    Install,
    /// Start COPE in background
    Start,
    /// Stop background COPE process
    Stop,
    /// Show current status
    Status,
    /// Remove COPE from startup and delete config
    Uninstall,
    /// Run as background daemon (internal)
    Daemon,
    /// Print help information
    Help,
}

impl Commands {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "install" => Some(Commands::Install),
            "start" => Some(Commands::Start),
            "stop" => Some(Commands::Stop),
            "status" => Some(Commands::Status),
            "uninstall" => Some(Commands::Uninstall),
            "help" => Some(Commands::Help),
            "daemon" => Some(Commands::Daemon),
            _ => None,
        }
    }
}

pub struct Cli {
    pub command: Commands,
}

impl Cli {
    pub fn parse() -> Self {
        // Simple CLI parsing: cope <command>
        let args: Vec<String> = std::env::args().skip(1).collect();
        let command = if args.is_empty() {
            // No command, print help and exit
            println!("{}", Self::help());
            std::process::exit(0);
        } else {
            Commands::from_str(&args[0]).unwrap_or(Commands::Status)
        };
        Self { command }
    }

    pub fn help() -> String {
        "COPE
route any CA. instantly.
built for the Solana trenches.

Usage:
  cope <COMMAND>

Commands:
  install      Install and start COPE
  start        Start COPE
  stop         Stop COPE
  status       Show COPE status
  uninstall    Remove COPE

Hotkeys:
  Alt+A        Axiom
  Alt+G        GMGN
  Alt+X        X Search
  Alt+D        DexScreener
  Alt+P        Pump.fun
  Alt+F        FOMO
  Alt+S        Solscan

Run 'cope help' for more information."
            .to_string()
    }
}

pub fn execute(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Start => cmd_start(),
        Commands::Stop => cmd_stop(),
        Commands::Status => cmd_status(),
        Commands::Uninstall => cmd_uninstall(),
        Commands::Install => cmd_install(),
        Commands::Help => Ok(()), // handled below
        Commands::Daemon => Ok(()),
    }
}

fn cmd_install() -> Result<()> {
    print_branding();
    print_routes_table();

    print!("\nStart with Windows? [Y/n] ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let start_with_windows =
        input.trim().is_empty() || input.trim().to_lowercase().starts_with('y');

    let mut config = Config::load().unwrap_or_default();

    // Ensure all 7 default routes exist (migrate from older configs)
    config.ensure_default_routes();

    config.start_with_windows = start_with_windows;

    if start_with_windows {
        enable_startup()?;
        println!("Added to Windows startup.");
    }

    config.save()?;

    println!("\nReady.");
    println!("COPE is running in the background.");
    println!("Highlight a CA, press Alt+A/G/X/D/P/S to route.");

    start_background()?;
    Ok(())
}

fn cmd_start() -> Result<()> {
    if is_daemon_running()? {
        println!("COPE is already running.");
        return Ok(());
    }

    let _config = Config::load().unwrap_or_default();
    start_background()?;
    println!("COPE started.");
    Ok(())
}

fn cmd_stop() -> Result<()> {
    if stop_daemon()? {
        println!("COPE stopped.");
    } else {
        println!("COPE is not running.");
    }
    Ok(())
}

fn cmd_status() -> Result<()> {
    let running = is_daemon_running()?;
    let startup = is_startup_enabled()?;
    let config = Config::load().unwrap_or_default();

    println!("COPE Status");
    println!("===========");
    println!("Running:     {}", if running { "Yes" } else { "No" });
    println!(
        "Startup:     {}",
        if startup { "Enabled" } else { "Disabled" }
    );
    println!("Config dir:  {}", config_dir()?.display());
    println!();
    print_routes_table_with_status(&config);
    Ok(())
}

fn cmd_uninstall() -> Result<()> {
    print!("Remove COPE from startup and delete config? [y/N] ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if !input.trim().to_lowercase().starts_with('y') {
        println!("Cancelled.");
        return Ok(());
    }

    let _ = stop_daemon();
    let _ = disable_startup();

    // Remove installed executable and COPE directory
    let install_dir = config_dir()?;
    if install_dir.exists() {
        let _ = std::fs::remove_dir_all(&install_dir);
    }

    // Remove from current user PATH
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("C:\\Users\\User\\AppData\\Local"));
    let cope_dir = local_app_data.join("COPE");
    remove_from_user_path(&cope_dir)?;

    // Remove config file if it still exists at old location
    let config_path = Config::config_path()?;
    if config_path.exists() {
        let _ = std::fs::remove_file(&config_path);
    }

    println!("COPE uninstalled.");
    Ok(())
}

fn print_branding() {
    println!("{}", COPE_WORDMARK);
    println!("{}", COPE_TAGLINE);
    println!("{}", COPE_MEMECOIN_TAGLINE);
    println!();
}

fn print_routes_table() {
    println!("{:<14} Alt+A", "Axiom");
    println!("{:<14} Alt+G", "GMGN");
    println!("{:<14} Alt+X", "X Search");
    println!("{:<14} Alt+D", "DexScreener");
    println!("{:<14} Alt+P", "Pump.fun");
    println!("{:<14} Alt+F", "FOMO");
    println!("{:<14} Alt+S", "Solscan");
}

fn print_routes_table_with_status(config: &Config) {
    println!("{:<14} {:<10} Status", "Destination", "Hotkey");
    println!("{}", "-".repeat(40));

    for dest in Destination::all() {
        let route = config.get_route(*dest).unwrap();
        let status = if route.enabled { "enabled" } else { "disabled" };
        println!(
            "{:<14} {:<10} {}",
            dest.display_name(),
            route.hotkey_string(),
            status
        );
    }
}
