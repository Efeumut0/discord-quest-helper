# Discord Quest Helper

[![Windows build](https://github.com/Efeumut0/discord-quest-helper/actions/workflows/windows.yml/badge.svg)](https://github.com/Efeumut0/discord-quest-helper/actions/workflows/windows.yml) [![Release](https://img.shields.io/github/v/release/Efeumut0/discord-quest-helper)](https://github.com/Efeumut0/discord-quest-helper/releases) [![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Discord Quest Helper creates small Windows executables for Discord play-time quests. It uses Discord's game catalog to find executable names and lets you create, start and stop helpers from the app.

[Türkçe kullanım kılavuzu](README.tr.md)

## What it does

- Search the game catalog and see newly added games.
- Create a helper for one game or several at once.
- Start and stop generated helpers, or open their folders.
- Test Rich Presence and view application logs.
- Choose Turkish or English and adjust the theme and search settings.

Source 1 uses executable names from Discord's catalog. Source 2 uses estimated names and RPC to show an activity on your profile. Profile activity does not confirm quest progress; check the quest in Discord. Detection and completion aren't guaranteed.

## Download and run

Download the Windows x64 ZIP from [Releases](https://github.com/Efeumut0/discord-quest-helper/releases), extract it into a writable folder, and open `DiscordQuestHelper.exe`. The helper doesn't ask for your Discord account token.

Requirements: Windows x64, Microsoft Edge WebView2 Runtime, and the Discord desktop app running and signed in when using Discord integration.

1. Accept a supported play-time quest in Discord.
2. Search for its game in Discord Quest Helper.
3. Prefer Source 1 when Discord provides a supported Windows executable.
4. Create and start the helper; check progress in Discord itself.
5. Stop the helper when finished. A visible RPC activity alone does not confirm progress.

Generated files are stored beside the app in `games/<application-id>/`. Runtime folders such as `metadata/`, `data/`, and `resources/` are created as needed. Keep them out of source control. The app does not include an automatic updater.

## Build from source

Build on Windows x64 with PowerShell 7, Node.js 22, Corepack, stable Rust, and Visual Studio C++ Build Tools with the Windows SDK. Run from the repository root:

```powershell
corepack enable
pwsh -File scripts/build.ps1
```

The script runs formatting checks, tests and Clippy, builds the runner, embeds it, and creates a portable Tauri release. Default output: `../discord-quest-helper-win32-x64/DiscordQuestHelper.exe` with `SHA256SUMS.txt`. Dated source folders preserve their date suffix in the sibling distribution folder.

```powershell
# Optional output outside the source tree
pwsh -File scripts/build.ps1 -OutputDirectory C:/builds/discord-quest-helper
```

Use the Tauri build script for distribution: `cargo build --release` alone does not build/embed the frontend correctly. No installer is produced.

## Project layout

| Directory | Purpose |
| --- | --- |
| `src/` | Vue UI, translations and TypeScript helpers |
| `src-tauri/` | Windows file generation, process management and catalog access |
| `src-win/` | Generated helper process; optional Source 2 RPC over local Discord IPC |
| `scripts/` | Build and clean-source export |
| `tests/` | Source selection and creation regression checks |
| `.github/` | Windows CI, issue templates and contribution support |

## Contributing and support

Use an issue to report a reproducible bug or request a feature. Include the app version, Windows/Discord version, game name and selected source. Redact account information, tokens and personal paths from logs.

Build instructions for contributors are in [CONTRIBUTING.md](CONTRIBUTING.md). Report vulnerabilities through [Security](https://github.com/Efeumut0/discord-quest-helper/security/advisories/new); see [SECURITY.md](SECURITY.md).

Maintained by [efeumut](https://github.com/Efeumut0).

## License and affiliation

MIT. This is an independent community project and is not affiliated with or endorsed by Discord. Discord and game names belong to their respective owners.
