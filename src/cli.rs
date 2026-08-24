use crate::config::{config_dir, Config};
use crate::history::History;
use crate::routes::Destination;
use crate::windows::{
    disable_startup, enable_startup, installed_cope_dir, is_daemon_running, is_startup_enabled,
    remove_cope_dir_if_empty, remove_from_user_path, remove_owned_data_files,
    schedule_deferred_cleanup, start_background, stop_daemon, verified_cope_dir,
};
use anyhow::Result;
use std::io::{self, Write};
use std::sync::Arc;

const COPE_TAGLINE: &str = "route Solana CAs with hotkeys.";
const COPE_MEMECOIN_TAGLINE: &str = "built for Solana traders.";

const C: &str = "\x1b[38;2;0;200;255m";
const G: &str = "\x1b[32m";
const R: &str = "\x1b[31m";
const Y: &str = "\x1b[33m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

#[derive(Clone, Debug)]
pub enum Commands {
    Install,
    Start,
    Stop,
    Status,
    Uninstall,
    History { all: bool, clear: bool },
    Daemon,
    Cleanup { parent_pid: u32 },
    Help,
    Version,
}

impl Commands {
    pub fn from_str(args: &[String]) -> Option<Self> {
        match args.first().map(|s| s.as_str()) {
            Some("install") => Some(Commands::Install),
            Some("start") => Some(Commands::Start),
            Some("stop") => Some(Commands::Stop),
            Some("status") => Some(Commands::Status),
            Some("uninstall") => Some(Commands::Uninstall),
            Some("history") => {
                let all = args.iter().any(|a| a == "--all");
                let clear = args.get(1).map(|s| s.as_str()) == Some("clear");
                Some(Commands::History { all, clear })
            }
            Some("help") => Some(Commands::Help),
            Some("daemon") => Some(Commands::Daemon),
            Some("__cope_cleanup") => args
                .get(1)
                .and_then(|pid| pid.parse().ok())
                .map(|parent_pid| Commands::Cleanup { parent_pid }),
            Some("--version") | Some("-v") => Some(Commands::Version),
            Some("--help") | Some("-h") => Some(Commands::Help),
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
            Commands::from_str(&args).unwrap_or(Commands::Status)
        };
        Self { command }
    }

    pub fn help() -> String {
        "COPE
route Solana CAs with hotkeys.
built for Solana traders.

Usage:
  cope <COMMAND>

Commands:
  install      Install and start COPE
  start        Start COPE
  stop         Stop COPE
  status       Show COPE status
  history      Show recent route history
  uninstall    Remove COPE

Hotkeys:
  Alt+A        Axiom
  Alt+G        GMGN
  Alt+X        X Search
  Alt+D        DexScreener
  Alt+P        Pump.fun
  Alt+F        FOMO
  Alt+S        Solscan
  Alt+Q        RugCheck
  Alt+B        Bundle Checker (Trench Radar clusters)

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
        Commands::History { all, clear } => cmd_history(all, clear),
        Commands::Cleanup { parent_pid } => crate::windows::run_deferred_cleanup(parent_pid),
        Commands::Help | Commands::Daemon | Commands::Version => Ok(()),
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

    installed_cope_dir()?;
    let cope_dir = verified_cope_dir()?;
    let installed_exe = cope_dir.join("cope.exe");

    if is_daemon_running()? {
        println!("{Y}COPE daemon is already running. Stopping it first...{RESET}");
        match stop_daemon() {
            Ok(true) => println!("{G}Existing daemon stopped.{RESET}"),
            Ok(false) => {}
            Err(e) => {
                eprintln!("{R}Cannot stop existing COPE daemon: {e}{RESET}");
                anyhow::bail!("Failed to stop existing daemon");
            }
        }
    }

    std::fs::create_dir_all(&cope_dir)?;
    std::fs::copy(&current_exe, &installed_exe)?;

    let cope_dir_str = cope_dir.to_string_lossy().to_string();
    let root = crate::windows::registry_root()?;
    let (paths_key, _) = root.create_subkey("Environment")?;
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

    if start_with_windows {
        enable_startup()?;
        println!("{G}Added to Windows startup.{RESET}");
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
        eprintln!("{R}COPE could not start.{RESET}");
        for msg in failed {
            eprintln!("{Y}{msg}{RESET}");
        }
        anyhow::bail!("Hotkey registration failed");
    }

    let mut config = Config::load().unwrap_or_default();
    config.ensure_default_routes();
    config.start_with_windows = start_with_windows;
    config.save()?;

    start_background(Some(installed_exe))?;

    println!();
    println!("{G}Ready.{RESET}");
    println!("{G}COPE is running in the background.{RESET}");

    Ok(())
}

fn cmd_start() -> Result<()> {
    // "Already running" is an expected idempotent state, not a failure:
    // start_background returns Ok(false) in that case.
    let started = start_background(None)?;
    if started {
        println!("{G}COPE started.{RESET}");
    } else {
        println!("{Y}COPE is already running.{RESET}");
    }
    Ok(())
}

fn cmd_stop() -> Result<()> {
    match stop_daemon() {
        Ok(true) => println!("{R}COPE stopped.{RESET}"),
        Ok(false) => println!("{DIM}COPE is not running.{RESET}"),
        Err(e) => {
            println!("{R}Failed to stop COPE: {e}{RESET}");
            return Err(e);
        }
    }
    Ok(())
}

fn cmd_status() -> Result<()> {
    let running = is_daemon_running()?;
    let startup = is_startup_enabled()?;
    let mut config = Config::load().unwrap_or_default();
    config.ensure_default_routes();

    println!("{C}COPE STATUS{RESET}");
    println!();
    let running_color = if running { G } else { R };
    let running_val = if running { "Yes" } else { "No" };
    println!("  {DIM}Running{RESET}    {running_color}{running_val}{RESET}");
    let startup_val = if startup { "Enabled" } else { "Disabled" };
    println!("  {DIM}Startup{RESET}    {C}{startup_val}{RESET}");
    println!("  {DIM}Installed{RESET}  {G}Yes{RESET}");
    if let Ok(dir) = config_dir() {
        println!("  {DIM}Data dir{RESET}  {DIM}{}{RESET}", dir.display());
    }

    println!();
    println!("{C}  {:<14} {:<10} Status{RESET}", "Destination", "Hotkey");
    println!("  {}", "-".repeat(38));

    for dest in Destination::all() {
        let route = match config.get_route(*dest) {
            Some(r) => r,
            None => continue,
        };
        let status = if route.enabled {
            format!("{G}enabled{RESET}")
        } else {
            format!("{DIM}disabled{RESET}")
        };
        println!(
            "  {:<14} {:<10} {}",
            dest.display_name(),
            route.hotkey_string(),
            status
        );
    }

    Ok(())
}

fn cmd_uninstall() -> Result<()> {
    print!("Remove COPE and its local data? [y/N] ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if !input.trim().to_lowercase().starts_with('y') {
        println!("Cancelled.");
        return Ok(());
    }

    if let Err(e) = stop_daemon() {
        eprintln!("{R}Cannot stop COPE daemon: {e}{RESET}");
        anyhow::bail!("Failed to stop existing daemon");
    }
    disable_startup()?;

    let cope_dir = installed_cope_dir()?;
    remove_from_user_path(&cope_dir)?;

    // Remove only files COPE owns. The running installed executable is
    // deferred to a native helper because Windows keeps its image open.
    remove_owned_data_files(&cope_dir)?;
    let installed_exe = cope_dir.join("cope.exe");
    if installed_exe.exists() {
        schedule_deferred_cleanup(&installed_exe)?;
    }
    if !installed_exe.exists() {
        let _ = remove_cope_dir_if_empty(&cope_dir)?;
    }

    println!("{R}COPE uninstalled.{RESET}");
    Ok(())
}

fn cmd_history(all: bool, clear: bool) -> Result<()> {
    let history = History::new()?;

    if clear {
        history.clear()?;
        println!("{G}History cleared.{RESET}");
        return Ok(());
    }

    println!("{C}COPE HISTORY{RESET}");
    println!();

    let entries = if all {
        history.read_all()?
    } else {
        history.read_latest()?
    };

    if entries.is_empty() {
        println!("  No route history yet.");
        return Ok(());
    }

    println!("  {DIM}#   {:<52} DATE / TIME{RESET}", "CONTRACT ADDRESS");
    println!("  {}", "-".repeat(72));

    for (i, entry) in entries.iter().enumerate() {
        let ca_display = if entry.ca.len() > 48 {
            let end_prefix = entry
                .ca
                .char_indices()
                .nth(24)
                .map_or(entry.ca.len(), |(i, _)| i);
            let start_suffix = entry.ca.len() - 20;
            let start_suffix = entry
                .ca
                .char_indices()
                .nth(start_suffix)
                .map_or(entry.ca.len(), |(i, _)| i);
            format!(
                "{}...{}",
                &entry.ca[..end_prefix],
                &entry.ca[start_suffix..]
            )
        } else {
            entry.ca.clone()
        };
        println!(
            "  {DIM}{:<4}{RESET} {:<52} {DIM}{}{RESET}",
            i + 1,
            ca_display,
            entry.timestamp
        );
    }

    if !all && entries.len() >= 25 {
        println!("\n  Showing latest 25. Use 'cope history --all' for more.");
    }

    Ok(())
}

fn print_branding() {
    println!("{}   ██████╗ ██████╗ ██████╗ ███████╗{}", C, RESET);
    println!("{}  ██╔════╝██╔═══██╗██╔══██╗██╔════╝{}", C, RESET);
    println!("{}  ██║     ██║   ██║██████╔╝█████╗{}", C, RESET);
    println!("{}  ██║     ██║   ██║██╔═══╝ ██╔══╝{}", C, RESET);
    println!("{}  ╚██████╗╚██████╔╝██║     ███████╗{}", C, RESET);
    println!("{}   ╚═════╝ ╚═════╝ ╚═╝     ╚══════╝{}", C, RESET);
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
    println!("{:<8} RugCheck", "Alt+Q");
    println!("{:<8} Bundle Checker (Trench Radar)", "Alt+B");
}
