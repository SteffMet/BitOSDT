# BitOSDT 2.0x

BitOSDT 2.0 is a comprehensive Windows deployment solution written in Rust, designed as a modern replacement for OSDCloud. It provides automated Windows deployment with hardware detection, driver management, and WinPE generation.

This project is now open-source. The original version faced a legal challenge from a large IT corporation over piracy concerns, which required significant adjustments to the codebase, licensing, and distribution model thus I decided to release it to the public. 

## Features

- **Image Management**: Create and manage Windows deployment images
- **Hardware Detection**: Automatic detection of system specifications
- **Driver Management**: Automatic DriverPack matching and installation
- **WinPE Builder**: Create custom WinPE boot media
- **Deployment Engine**: Full disk partitioning and Windows deployment
- **Cross-Platform**: Core functionality works on Windows and Linux (development)

## Custom Installer Sources

The image wizard supports multiple custom installer source modes for post-deployment app installs:

- **Embedded file (Full ISO only)**: Select local `MSI`, `MSIX`, or `EXE` files and BitOSDT stages them into the image at `C:\BitOSDT\Installers\...`.
- **UNC directory**: Configure a network directory (`\\server\share\folder`) plus installer filename. These installers are deferred to first admin logon.
- **Direct path/URL**: Use an existing local path or HTTP/HTTPS URL directly.

For UNC directory installs:

- BitOSDT writes a deferred script to `C:\Windows\Setup\Scripts\Install-NetworkApps.ps1`.
- The deferred script prompts for network credentials at first admin logon (credentials are not persisted by BitOSDT).
- Install logs are written to `C:\BitOSDT\Logs\app-install-network.log`.

Lightweight behavior:

- `FullISO` applies embedded and deferred network custom installer logic.
- `LightweightISO` currently does **not** execute custom installer payload staging/deferred flow.
- `Both` applies the custom installer behavior to the Full ISO output only.

## System Requirements

### Windows (Production)
- Windows 10/11 or Windows Server 2019/2022
- Windows ADK (Assessment and Deployment Kit)
- Windows ADK WinPE add-on
- Administrative privileges

### Linux (Development)
- Ubuntu 20.04+ or similar
- Rust 1.70+
- Node.js 18+ (for GUI)

## Installation

### From Source

1. **Clone the repository:**
   ```bash
   git clone https://github.com/bitosdt/bitosdt.git
   cd bitosdt/cargo
   ```

2. **Build the project:**
   ```bash
   cargo build --release
   ```

3. **Initialize BitOSDT:**
   ```bash
   ./target/release/bitosdt init
   ```

### Windows ADK Setup

1. Download Windows ADK from Microsoft
2. Install ADK and WinPE add-on
3. Default installation path: `C:\Program Files (x86)\Windows Kits\10\`

## Quick Start

### 1. List Available OS Versions
```bash
bitosdt os-list
```

### 2. Create an Image
```bash
bitosdt create-image --name "Windows 11 Pro" --os win11-24h2 --license pro
```

### 3. Detect Hardware
```bash
bitosdt hardware
```

### 4. Build WinPE
```bash
bitosdt build-winpe --output mywinpe --arch x64
```

### 5. Deploy Windows
```bash
bitosdt deploy --image <image-id> --disk 0 --uefi
```

### WinPE Deployment Troubleshooting

If full-ISO deployment appears stuck in WinPE (for example at `diskpart.exe`), use:

```cmd
type X:\BitOSDT\Logs\deploy.log
type X:\BitOSDT\Logs\partition.txt
diskpart /s X:\BitOSDT\Logs\partition.txt
```

The deploy script now writes command stdout/stderr capture files in `X:\BitOSDT\Logs\` for deeper diagnostics.

### Bundled WinPE Packages (Chromium)

BitOSDT now bundles `WinPE-Dependencies/Packages/**` into the Tauri app resources and copies it into WinPE at:

- `X:\BitOSDT\Packages`

Chromium payload expectations:

- Required source path: `WinPE-Dependencies/Packages/chrome/chrome.exe`
- WinPE launchers written during build:
- `X:\BitOSDT\Scripts\Launch-Chromium.ps1`
- `X:\BitOSDT\Scripts\Launch-Chromium.cmd`

Launch behavior in WinPE:

- Manual launch only (no auto-start).
- Launcher attempts normal sandbox mode first.
- If Chromium exits quickly or fails to start, launcher retries with `--no-sandbox`.
- Launcher logs to `X:\BitOSDT\Logs\chromium-launch.log`.
- Profile directory: `X:\BitOSDT\State\chrome-profile`.

Size impact:

- Bundling Chromium currently adds roughly `~657 MB` from `WinPE-Dependencies/Packages/chrome`.

## CLI Commands

| Command | Description |
|---------|-------------|
| `init` | Initialize BitOSDT configuration |
| `os-list` | List available Windows versions |
| `create-image` | Create a new deployment image |
| `list-images` | List all images |
| `hardware` | Detect system hardware |
| `match-drivers` | Find matching DriverPacks |
| `sync-catalogs` | Sync driver catalogs |
| `build-winpe` | Build WinPE boot media |
| `list-usb` | List USB devices |
| `deploy` | Deploy image to disk |
| `info` | Show system information |

## Project Structure

```
bitosdt/
├── cargo/              # Rust backend
│   ├── src/
│   │   ├── core/       # Database, config, models
│   │   ├── catalog/    # Driver catalog management
│   │   ├── build/      # WinPE/ISO/USB creation
│   │   ├── deploy/     # Deployment engine
│   │   ├── ui/         # Tauri commands
│   │   └── utils/      # Utilities
│   └── src-tauri/      # Tauri config
├── BitOSDT/2.0/        # Architecture documentation
└── README.md
```

## Architecture

BitOSDT follows a modular architecture:

1. **Core Module**: Database (SQLite), configuration, error handling
2. **Catalog Module**: OSDCloud XML sync, DriverPack management
3. **Build Module**: WinPE creation, ISO generation, USB writing
4. **Deploy Module**: Hardware detection, disk operations, WIM management
5. **UI Module**: Tauri-based GUI (optional)

## Configuration

On Windows, configuration defaults to `C:\BitOSDT\config.json`:

```json
{
  "settings": {
    "default_language": "en-US",
    "theme": "system",
    "auto_check_updates": true,
    "download_path": "C:\\BitOSDT\\Downloads",
    "workspace_path": "C:\\BitOSDT\\Workspace"
  },
  "database_path": "C:\\BitOSDT\\bitosdt.db"
}
```

Linux-hosted workflows continue using `~/.bitosdt/...` defaults where required.

## Development

### Running Tests
```bash
cargo test
```

### Building GUI (requires Tauri)
```bash
cargo build --features gui
```

### Code Formatting
```bash
cargo fmt
cargo clippy
```

## Roadmap

- [x] Phase 1: Foundation (Rust + Tauri + Database)
- [x] Phase 2: Image Management (OS Catalog, Downloads)
- [x] Phase 3: Hardware Detection (WMI, Linux fallback)
- [x] Phase 4: Driver System (DriverPack matching)
- [x] Phase 5: WinPE Builder (ADK integration)
- [x] Phase 6: Deployment Engine (DISM, disk operations)
- [ ] Phase 7: Testing & QA
- [ ] Phase 8: Documentation
- [ ] Phase 9: Release

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests
5. Submit a pull request

## License

MIT License - See LICENSE file for details

## Acknowledgments

- Inspired by OSDCloud by David Segura
- Built with Rust, Tauri, and React
- Uses Windows ADK for WinPE and deployment
