# WinCare

Native Windows 10/11 maintenance, diagnostics, network tuning, and registry debloat utility written in Rust (`egui`/`eframe`). Single standalone executable, no runtime dependencies.

Bilingual support (`English` / `Français`) is auto-detected from the Windows locale, or switchable via the UI toggle (`[EN | FR]`) and the `--lang en|fr` command-line argument.

## Features

- **System Repair**: DISM (`CheckHealth`, `ScanHealth`, `RestoreHealth`, `/StartComponentCleanup /ResetBase`), SFC (`sfc /scannow` with UTF-16LE decoding), CHKDSK, Windows Update service and `SoftwareDistribution`/`catroot2` reset, network stack reset (Winsock, TCP/IP, DNS, WinHTTP proxy, firewall), AppX/Store re-registration, and critical service recovery.
- **PC Diagnostics & Maintenance**: Real-time hardware and OS metrics, per-directory cache scanner, Top 15 largest `%TEMP%` subdirectory analyzer with targeted cleanup, Windows Task Manager-compatible `StartupApproved` startup item manager, System Restore point creation (`Checkpoint-Computer`), and before/after comparison.
- **Network Diagnostics & Tuning**: Active route and adapter inspection, Wi-Fi signal metrics, DNS latency benchmark (Cloudflare, Google, Quad9, OpenDNS, NextDNS) with automatic NextDNS DoH/DoT preservation, non-fragmenting MTU discovery (`ping -f -l`), TCP connection table by process, and TCP/IP stack tuning (`netsh int tcp`).
- **Windows 11 Registry Debloat**: 18 reversible registry policies (Copilot, Recall, Click to Do, Telemetry, Bing Search, Widgets, classic Windows 10 context menu, taskbar End Task, mouse acceleration, Fast Startup) with live registry state inspection.
- **Software Updates & Bloatware Removal (v0.4.0)**:
  - **WinGet Updater**: Interrogates official Windows Package Manager repositories, listing current vs available versions with selective single-package updates or unattended silent batch upgrades (`winget upgrade --all`).
  - **AppX Bloatware Uninstaller**: Detects and purges 14 superfluous pre-installed Windows 10/11 applications (Clipchamp, Cortana, Bing News/Weather, Solitaire, Mixed Reality, Teams Personal, etc.) along with provisioned system packages.
- **Gaming, Low-Latency & Hardware Health (v0.4.0)**:
  - **GPU Shader Cache Purge**: Clears DirectX `D3DSCache`, NVIDIA (`DXCache`, `GLCache`), and AMD (`DxCache`, `OglCache`) compiled shader directories to resolve frame stuttering and corrupted assets.
  - **Ultimate Performance Power Plan**: Unlocks and activates the hidden OEM Ultimate Performance power scheme (`e9a42b02-d5df-448d-aa00-03f14749eb61`).
  - **Gaming Switches**: Toggles for Hardware-Accelerated GPU Scheduling (HAGS), Virtualization-Based Security (VBS / Core Isolation), and Windows Game Mode with safety warnings.
  - **Battery & S.M.A.R.T. Health**: Evaluates battery design vs actual full charge capacity with real health %, generates official Windows HTML battery reports (`powercfg /batteryreport`), and inspects physical storage S.M.A.R.T. health, temperatures, wear %, and read/write error counters.
- **Automation & Configuration Profiles (v0.4.0)**:
  - **Scheduled Weekly Maintenance**: One-click Windows Task Scheduler registration (`WinCare_Weekly_Maintenance`) running background maintenance every Sunday at 12:00 PM without elevation prompts.
  - **JSON Configuration Profiles**: Export and import portable JSON configuration profiles (`wincare-profile.json`) to replicate repair sets, PC cleanup preferences, network parameters, and applied debloat policies across multiple PCs.
- **Reversibility & Safety**: Dedicated Restore tab to revert power plans, visual effects, DHCP DNS, MTU 1500, and registry policies back to Windows defaults; `--dry-run` simulation mode; full session logs written to `%LOCALAPPDATA%\WinCare\logs\WinCare_YYYY-MM-DD_HH-MM-SS.log`.

## Usage

Download `WinCare.exe` from [GitHub Releases](https://github.com/yaniswav/win-care/releases/latest). An integrated auto-updater is available in the application (`Check for updates`) and through the command-line interface (`--update`).

### Command-Line Interface

```powershell
# Launch graphical interface (prompts for UAC elevation automatically)
WinCare.exe

# Language selection
WinCare.exe --lang en
WinCare.exe --lang fr

# Read-only diagnostics (PC and network)
WinCare.exe --diagnose

# Automated maintenance tasks
WinCare.exe --auto          # Windows system repair
WinCare.exe --boost-pc      # Cache cleanup and safe system optimizations
WinCare.exe --boost-net     # Network stack optimization and MTU tuning
WinCare.exe --all-auto      # Sequentially run repair, PC boost, and network boost

# Utility and execution flags
WinCare.exe --update        # Check for and apply updates from GitHub Releases
WinCare.exe --dry-run       # Simulate operations without applying system modifications
WinCare.exe --no-elevate    # Run without requesting administrator elevation
```

## Building from Source

### Prerequisites

- Windows 10 or 11
- Rust stable MSVC toolchain (`rustup default stable-x86_64-pc-windows-msvc`)

### Build Steps

```powershell
git clone https://github.com/yaniswav/win-care.git
cd win-care

# Run unit and integration tests
cargo test

# Compile release binary
cargo build --release
```
