# COPE

route any CA. instantly.

built for the Solana trenches.

Highlight any Solana CA.
Press where you want to go.
COPE opens it in your Windows default browser.

## Routes

| Hotkey   | Destination | Notes |
|----------|-------------|-------|
| Alt+A    | Axiom       | Direct token URL |
| Alt+G    | GMGN        | Direct token URL |
| Alt+X    | X Search    | Direct token URL |
| Alt+D    | DexScreener | Direct token URL |
| Alt+P    | Pump.fun    | Direct token URL |
| Alt+F    | FOMO        | Direct token URL |
| Alt+S    | Solscan     | Direct token URL |
| Alt+Q    | RugCheck    | Direct token URL |
| Alt+B    | Bundle Checker | Trench Radar clusters deep-link for this CA |

## Install

1. Download `cope-windows-x64.exe` from GitHub Releases.
2. Open PowerShell in the download folder.
3. Run:

   .\cope-windows-x64.exe install

4. Done.

After installation, a new terminal can use:

- `cope status` - show running state
- `cope start` - start COPE daemon
- `cope stop` - stop COPE daemon
- `cope uninstall` - remove COPE
- `cope history` - show recent successful routes
- `cope history --all` - show all readable history entries
- `cope history clear` - clear local history

## Usage

- Highlight any Solana contract address (CA) and press the associated hotkey to route it.
- Or manually copy a CA to the clipboard and press the hotkey - COPE will extract it.
- Each hotkey opens the selected destination URL in your Windows default browser.
- A single newly selected valid CA becomes the sticky current CA. Empty or ambiguous
  selections do not replace it; with no new selection, hotkeys reuse the sticky CA.
- COPE restores the clipboard after selection capture and records only successful
  browser dispatches in local JSONL history.

## Commands

| Command      | Description                     |
|--------------|---------------------------------|
| `cope install`    | Install COPE for current user   |
| `cope start`      | Start COPE in background        |
| `cope stop`       | Stop COPE daemon                |
| `cope status`     | Show COPE status                |
| `cope uninstall`  | Remove COPE from system         |
| `cope --help`     | Print help                      |
| `cope --version`| Print version                   |

## Privacy

COPE is local-only software with no cloud dependencies:

- **local only** - all processing happens on your machine
- **no wallet connection** - never asks for seed phrases or private keys
- **no account** - no registration or login required
- **no telemetry** - no data sent to any COPE server
- **no analytics** - no usage tracking or monitoring
- **no keylogging** - ordinary keyboard input is not captured
- **no clipboard history** - selection capture only when a COPE hotkey is pressed
- **selection capture only when a COPE hotkey is pressed** - no continuous monitoring

COPE locally extracts and validates plausible Solana addresses from selected text.
It does not verify on-chain memecoin status, guarantee safe tokens, guarantee rug
detection, or guarantee bundle detection at any destination. The Bundle Checker
route opens Trench Radar's token-specific clusters page.

Configuration, PID state, and history are stored under `%LOCALAPPDATA%\COPE\`.
If Windows does not provide `LOCALAPPDATA`, COPE fails safely instead of writing to
a guessed user path.

## Source structure

```
src/main.rs      entry point + ANSI support
src/cli.rs       command-line interface
src/hotkeys.rs   global hotkeys + selection capture
src/parser.rs    Solana CA validation
src/routes.rs    destination URL routing
src/windows.rs   install/startup/process integration
src/history.rs   local route history
src/config.rs    configuration persistence
```

## Build from source

```bash
cargo build --release
```

The release binary will be at `target/release/cope.exe`.

## License

MIT

Copyright (c) 2026 COPE
