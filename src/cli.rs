use crate::config::{config_dir, Config};
use crate::routes::Destination;
use crate::windows::{
    disable_startup, enable_startup, installed_cope_dir, is_daemon_running, is_startup_enabled,
    remove_from_user_path, start_background, stop_daemon,
};
use anyhow::Result;
use std::io::{self, Write};
use std::sync::Arc;
use winreg::enums::KEY_CREATE_SUB_KEY;
use winreg::enums::KEY_READ;
use winreg::enums::KEY_WRITE;
use winreg::RegKey;

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
        let args: Vec<String> = std::env::args().skip(1).collect();
        let command = if args.is_empty() {
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
        Commands::Help => Ok(()),
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

    let current_exe = std::env::current_exe()?;

    let cope_dir = installed_cope_dir()?;
    let installed_exe = cope_dir.join("cope.exe");

    std::fs::create_dir_all(&cope_dir)?;
    std::fs::copy(&current_exe, &installed_exe)?;

    let cope_dir_str = cope_dir.to_string_lossy().to_string();
    let hkcu = RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    if let Ok(paths_key) =
        hkcu.open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE | KEY_CREATE_SUB_KEY)
    {
        let current_path = paths_key
            .get_value::<String, _>("PATH")
            .unwrap_or_else(|_| String::new());
        let dir_lower = cope_dir_str.to_lowercase();
        let already_present = current_path
            .split(';')
            .any(|e| e.trim().to_lowercase() == dir_lower);
        if !already_present {
            paths_key.set_value("PATH", &format!("{};{}", current_path, cope_dir_str))?;
        }
    }

    if start_with_windows {
        enable_startup()?;
        println!("Added to Windows startup.");
    }

    let failed = {
        let saved_level = log::max_level();
        log::set_max_level(log::LevelFilter::Warn);
        let check_config = Arc::new(std::sync::RwLock::new(Config::default()));
        let mut check_manager = crate::hotkeys::HotkeyManager::new(check_config)?;
        check_manager.register_hotkeys()?;
        let f = check_manager.get_failed_registrations().to_vec();
        drop(check_manager);
        log::set_max_level(saved_level);
        f
    };

    if !failed.is_empty() {
        let _ = disable_startup();
        let _ = remove_from_user_path(&cope_dir);
        let _ = std::fs::remove_file(&installed_exe);
        let _ = std::fs::remove_dir_all(&cope_dir);
        eprintln!("COPE could not start.");
        for msg in failed {
            eprintln!("{}", msg);
        }
        anyhow::bail!("Hotkey registration failed");
    }

    let mut config = Config::load().unwrap_or_default();
    config.ensure_default_routes();
    config.start_with_windows = start_with_windows;
    config.save()?;

    start_background(Some(installed_exe))?;

    println!("\nReady.");
    println!("COPE is running in the background.");

    Ok(())
}

fn cmd_start() -> Result<()> {
    if is_daemon_running()? {
        println!("COPE is already running.");
        return Ok(());
    }

    let _config = Config::load().unwrap_or_default();
    start_background(None)?;
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

    let cope_dir = installed_cope_dir()?;
    remove_from_user_path(&cope_dir)?;

    if cope_dir.exists() {
        let _ = std::fs::remove_dir_all(&cope_dir);
    }

    let config_path = Config::config_path()?;
    if config_path.exists() {
        let _ = std::fs::remove_file(&config_path);
    }

    println!("COPE uninstalled.");
    Ok(())
}

fn print_branding() {
    let c = "\x1b[38;2;0;200;255m";
    let r = "\x1b[0m";
    println!("{}   ██████╗ ██████╗ ██████╗ ███████╗{}", c, r);
    println!("{}  ██╔════╝██╔═══██╗██╔══██╗██╔════╝{}", c, r);
    println!("{}  ██║     ██║   ██║██████╔╝█████╗{}", c, r);
    println!("{}  ██║     ██║   ██║██╔═══╝ ██╔══╝{}", c, r);
    println!("{}  ╚██████╗╚██████╔╝██║     ███████╗{}", c, r);
    println!("{}   ╚═════╝ ╚═════╝ ╚═╝     ╚══════╝{}", c, r);
    println!("{}", COPE_TAGLINE);
    println!("{}", COPE_MEMECOIN_TAGLINE);
    println!();
}

fn print_routes_table() {
    println!("ROUTES\n");
    println!("{:<8} Axiom", "Alt+A");
    println!("{:<8} GMGN", "Alt+G");
    println!("{:<8} X Search", "Alt+X");
    println!("{:<8} DexScreener", "Alt+D");
    println!("{:<8} Pump.fun", "Alt+P");
    println!("{:<8} FOMO", "Alt+F");
    println!("{:<8} Solscan", "Alt+S");
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
