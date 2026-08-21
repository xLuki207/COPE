# COPE

route any CA. instantly.

built for the Solana trenches.

Highlight any Solana CA.
Press where you want to go.
COPE opens it in your Windows default browser.

## Routes

| Hotkey   | Destination |
|----------|-------------|
| Alt+G    | GMGN        |
| Alt+X    | X Search    |
| Alt+D    | DexScreener |
| Alt+P    | Pump.fun    |
| Alt+F    | FOMO        |
| Alt+S    | Solscan     |

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

## Usage

- Highlight any Solana contract address (CA) and press the associated hotkey to route it.
- Or manually copy a CA to the clipboard and press the hotkey - COPE will extract it.
- Each hotkey opens the selected destination URL in your Windows default browser.

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
It does not verify on-chain memecoin status or guarantee token support at any destination.

## Build from source

```bash
cargo build --release
```

The release binary will be at `target/release/cope.exe`.

## License

MIT

Copyright (c) 2026 COPE