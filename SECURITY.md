# COPE Security Model

COPE is a local-only Windows utility for routing Solana memecoin contract addresses (CAs) to destination websites. This document describes the trust model and what COPE does and does not do.

## What COPE Does

- **Locally extracts Solana addresses** - COPE checks clipboard text or user selection for plausible Solana contract addresses (32-44 character base58 strings). This extraction happens entirely on the client.

- **Opens URLs in default browser** - When a CA is identified, COPE constructs a URL and opens it in your Windows default browser. No on-chain verification is performed.

- **Global hotkey handling** - COPE only attempts text capture when a configured global hotkey (Alt+G/X/D/P/F/S) is pressed. It does not continuously monitor the clipboard or keyboard.

- **Per-user installation** - COPE installs to `%LOCALAPPDATA%\COPE\` and modifies only the current user's PATH and startup registry. No administrator permissions are required.

- **No account system** - COPE does not require or store any user accounts.

- **No telemetry or analytics** - COPE does not send any user data, usage statistics, or crash reports to any server.

- **No wallet integration** - COPE does not connect to, read, or interact with any cryptocurrency wallet.

- **No browser cookie access** - COPE does not read browser cookies, local storage, or session data.

- **No keylogging** - COPE does not record or monitor ordinary keyboard input except for the specific hotkey press detection.

## What COPE Does NOT Do

- **Does not access wallets** - COPE never reads seed phrases, private keys, or wallet files.

- **Does not request seed phrases or private keys** - The UI contains no fields for seed phrases or private keys.

- **Does not read browser cookies** - COPE has no access to browser data.

- **Does not continuously monitor clipboard** - COPE only reads the clipboard when a COPE hotkey is actively pressed. The clipboard is snapshot before and after Ctrl+C synthesis, then restored immediately.

- **Does not maintain clipboard history** - COPE restores the user's clipboard to its state before COPE's operation immediately after capture.

- **Does not contain telemetry/analytics** - There is no remote data collection of any kind.

- **Does not require an account** - COPE can be installed and used immediately without account creation.

- **Does not send user data to a COPE server** - All processing is local. The only network action is opening a URL in the default browser.

- **Does not verify on-chain memecoin status** - COPE locally extracts plausible Solana addresses from text. It does not query any API to verify whether a token is a legitimate memecoin or is supported by any destination.

## Vulnerability Reporting

If you believe you have found a security issue in COPE, please report it by opening an issue in the GitHub repository. Please include:

- A clear description of the issue
- Steps to reproduce
- Your Windows version and COPE version
- Any relevant error messages or output

We will investigate all reports and respond appropriately.

## Trust Summary

COPE is designed with privacy as the default. It processes text locally, does not require network access beyond opening URLs, and contains no hidden data collection. All COPE operations are triggered explicitly by the user via global hotkeys.