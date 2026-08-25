# COPE

A small Windows utility for routing Solana contract addresses with hotkeys.

Highlight a Solana CA, press a hotkey, and COPE opens the destination in your
default browser. No dashboard, extension, wallet, account, or telemetry.

## Routes

| Hotkey | Destination | URL |
| --- | --- | --- |
| Alt+A | Axiom | Token page |
| Alt+G | GMGN | Token page |
| Alt+X | X Search | Search for the CA |
| Alt+D | DexScreener | Token page |
| Alt+P | Pump.fun | Token page |
| Alt+F | FOMO | Token page |
| Alt+S | Solscan | Token page |
| Alt+Q | RugCheck | Token page |
| Alt+B | Bundle Checker / Trench Radar | CA clusters page |

## Install

1. Download `cope-windows-x64.exe` from GitHub Releases.
2. Open PowerShell and go to your Downloads folder:

   ```powershell
   cd $HOME\Downloads
   ```

3. Install COPE:

   ```powershell
   .\cope-windows-x64.exe install
   ```

After installation, open a new terminal to use the `cope` command.

## Usage

- Highlight a Solana contract address (CA) and press its hotkey.
- You can also copy a CA to the clipboard before pressing the hotkey.
- The latest valid CA that was either copied or selected becomes the current CA.
  Empty or ambiguous selections do not replace it.
- COPE restores the clipboard after selection capture and records successful
  routes in local JSONL history. History timestamps are shown and stored in EST
  (Eastern Standard Time, UTC-5).

## Commands

| Command | Description |
| --- | --- |
| `cope install` | Install COPE for the current user |
| `cope start` | Start the background daemon |
| `cope stop` | Stop the daemon |
| `cope status` | Show status and enabled routes |
| `cope uninstall` | Remove COPE and its local state |
| `cope history` | Show recent successful routes |
| `cope history --all` | Show all readable history entries |
| `cope history clear` | Clear local history |
| `cope --help` | Print help |
| `cope --version` | Print version |

## Privacy

COPE is local-only software with no cloud dependencies:

- **local only** - all processing happens on your machine
- **no wallet connection** - never asks for seed phrases or private keys
- **no account** - no registration or login required
- **no telemetry** - no data sent to any COPE server
- **no keylogging** - ordinary keyboard input is not captured
- **no continuous clipboard monitoring** - selection capture happens only when a
  COPE hotkey is pressed

COPE locally extracts and validates plausible Solana addresses from selected text.
It does not verify on-chain memecoin status, guarantee safe tokens, guarantee rug
detection, or guarantee bundle detection at any destination. The Bundle Checker
route opens Trench Radar's token-specific clusters page.

Configuration, PID state, and history are stored under `%LOCALAPPDATA%\COPE\`.
If Windows does not provide `LOCALAPPDATA`, COPE fails safely instead of writing to
a guessed user path.

## Build from source

```bash
cargo build --release
```

The release binary will be at `target/release/cope.exe`.

## License

MIT

Copyright (c) 2026 COPE
