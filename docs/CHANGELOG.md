# Changelog

All notable changes to **TrayDrop** are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- _placeholder for upcoming features_

## [1.0.0] - 2026-09-26

### Added
- Single-instance guard: named mutex (Windows) / `flock` + PID file (POSIX).
- Re-launching the shortcut brings the existing command window forward
  (`SetForegroundWindow` / `SIGUSR1` + `xdotool`) instead of starting twice.
- Notification-tray icon via **pystray** with *Show / Hide / About / Quit* menu;
  icon rendered at runtime with Pillow (no binary assets).
- Console auto-hides on start → app runs in the background.
- Interactive console REPL: `show`, `hide`, `status`, `quit`.
- Launchers: `launch_traydrop.bat`, silent VBS launcher, PowerShell desktop
  shortcut installer (`make_shortcut.ps1`), Linux shell launcher + `.desktop` entry.
- Documentation suite: README, User Guide, Setup Guide, Contributing, Code of
  Conduct, changelog, MIT license.
- Packaging (`pyproject.toml`, PEP 621) with `traydrop` console-script entry point.
- Unit tests for single-instance behaviour and icon rendering.

[Unreleased]: https://github.com/yourname/traydrop/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/yourname/traydrop/releases/tag/v1.0.0
