# COPE Security Model

COPE v0.1.0 — local-only Windows utility for routing Solana contract addresses.

## Supported version

Security fixes apply to the latest release only.

## Vulnerability reporting

If you believe you have found a security issue in COPE, please report it
privately by opening a GitHub Security Advisory at:

https://github.com/xLuki207/COPE/security/advisories/new

Please include:

- A clear description of the issue
- Steps to reproduce
- Your Windows version and COPE version
- Any relevant error messages or output

We will investigate all reports and respond appropriately. Please do not
disclose vulnerabilities publicly until a fix is available.

## What COPE does

- Registers nine global hotkeys (Alt+A/G/X/D/P/F/S/Q/B) via the Windows API
- On hotkey press: snapshots clipboard, injects Ctrl+C, reads selected text
- Extracts valid Solana addresses (Base58, 32-byte decode) from selected text
- Constructs a fixed HTTPS URL and opens it in the default browser
- Restores the previous clipboard
- Optionally stores local route history (CA + destination + timestamp)
- The Bundle Checker route opens Trench Radar's token-specific clusters page;
  COPE does not analyze the token itself

## What COPE does not do

- **No wallet access** — COPE never reads seed phrases, private keys, or wallet files
- **No backend** — COPE has no server, no cloud, no API calls
- **No telemetry** — No data is sent to any COPE server
- **No analytics** — No usage tracking or monitoring
- **No continuous clipboard monitoring** — Clipboard is accessed only when a COPE hotkey is pressed
- **No clipboard persistence** — Clipboard content is never written to disk
- **No keylogging** — Ordinary keyboard input is not captured
- **No arbitrary command execution** — Selected text is parsed, never executed
- **No browser automation** — COPE asks Windows to open a URL; the browser handles the rest
- **No transaction signing** — COPE cannot initiate trades or transfers
- **No account system** — No registration or login required

## Threat model

COPE is designed for single-user desktop use on Windows. An attacker who can:

- Write to `%LOCALAPPDATA%\COPE\` could replace the COPE binary
- Modify the Windows registry `HKCU\...\Run` key could alter startup behavior
- Control the clipboard at the exact moment of a hotkey press could influence URL destination (limited by Base58 validation)

These are standard desktop trust assumptions. COPE does not elevate privileges.
Destination analysis is informational; COPE does not guarantee a token is safe,
unruggable, or free of bundled supply.

## Build provenance

The release binary is built from source using:

- `cargo build --locked --release`
- `Cargo.lock` is committed
- `cargo audit` reports zero advisories
- `cargo clippy --all-targets --all-features -- -D warnings` passes
- All 39 unit tests and 4 lifecycle tests pass

Release binary checksums are published with the GitHub release.

## Limitations

- COPE validates Solana addresses locally but does not verify on-chain status
- The "sticky CA" feature reuses the last valid address when no new selection is detected
- History is stored in plaintext JSONL in the config directory
- Configuration, history, and PID state are stored under `%LOCALAPPDATA%\\COPE\\`
