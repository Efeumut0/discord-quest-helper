# Contributing

For a pull request, explain what changes for the user and how you tested it. Keep generated executables, local metadata and logs out of commits.

## Checks

On Windows with the prerequisites listed in README, run:

```powershell
pwsh -File scripts/build.ps1 -Unsigned
```

This checks formatting, runs runner/backend and release-tool tests, checks Clippy, and builds a test installer. Official release builds require the private updater signing key; contributor builds use `-Unsigned`. For behavior changes, include a regression test that exercises the affected flow. Explain any checks you could not run.

Source 1 uses catalog executable names; Source 2 uses RPC for profile display. Keep that distinction clear in code and UI text.

## Issues and pull requests

Provide reproduction steps, expected/actual behavior and the relevant game/source. Screenshots and logs should be redacted. Do not include account tokens, private keys or personal data. Keep upstream license notices intact.
