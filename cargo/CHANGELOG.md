# Changelog

All notable changes to BitOSDT will be documented in this file.

## [2.0.0] - 2026-02-03

### Added
- Complete Rust rewrite from PowerShell OSDCloud
- Modular architecture with separate crates for core, catalog, build, and deploy
- SQLite database for image and configuration storage
- Hardware detection via WMI (Windows) and /sys (Linux)
- Automatic DriverPack matching based on manufacturer/model
- WinPE builder with ADK integration
- WIM deployment using DISM
- Cross-platform support (Windows production, Linux development)
- CLI with comprehensive commands
- Configuration management with JSON persistence

### Core Features
- **Image Management**: Create, list, and manage Windows deployment images
- **OS Catalog**: Built-in catalog of Windows 10/11 versions with download URLs
- **Hardware Detection**: CPU, memory, disk, network, BIOS, TPM detection
- **Driver Management**: OSDCloud catalog sync, DriverPack download and extraction
- **WinPE Builder**: Create bootable WinPE with custom drivers and PowerShell
- **Disk Operations**: GPT/MBR partitioning, secure wiping
- **WIM Operations**: Apply, capture, export, split WIM files
- **Bootloader**: UEFI and BIOS bootloader configuration
- **Deployment Engine**: End-to-end deployment with progress tracking

### Technical
- Rust 1.70+ with tokio async runtime
- SQLite with rusqlite for data persistence
- Serde for serialization
- Clap for CLI parsing
- Tracing for logging
- Thiserror for error handling

### CLI Commands
- `init` - Initialize configuration
- `os-list` - List available OS versions
- `create-image` - Create deployment image
- `list-images` - List all images
- `hardware` - Detect hardware
- `match-drivers` - Find matching drivers
- `sync-catalogs` - Sync driver catalogs
- `build-winpe` - Build WinPE media
- `list-usb` - List USB devices
- `deploy` - Deploy image to disk
- `info` - Show system info

### Dependencies
- tokio 1.35+ (async runtime)
- rusqlite 0.30+ (SQLite)
- reqwest 0.11+ (HTTP client)
- serde 1.0+ (serialization)
- clap 4.4+ (CLI)
- chrono 0.4+ (datetime)
- uuid 1.6+ (identifiers)
- md5 0.7+ (hash verification)
- quick-xml 0.31+ (XML parsing)

### Development
- Cross-platform testing framework
- Mock implementations for Linux development
- Comprehensive error handling
- Progress tracking and reporting

## Future Plans

### Version 2.1.0 (Planned)
- [ ] GUI with Tauri + React
- [ ] Cloud driver support (post-deployment)
- [ ] Autopilot integration
- [ ] Task sequences
- [ ] Deployment reporting
- [ ] Multi-language support

### Version 2.2.0 (Planned)
- [ ] Group policy integration
- [ ] Domain join automation
- [ ] User profile migration
- [ ] BitLocker support
- [ ] Network deployment (PXE)

## Migration from OSDCloud

BitOSDT 2.0 is designed as a drop-in replacement for OSDCloud PowerShell:

| OSDCloud | BitOSDT 2.0 |
|----------|-------------|
| `Start-OSDCloud` | `bitosdt deploy` |
| `Get-OSDCloudDriverPack` | `bitosdt match-drivers` |
| `New-OSDCloudWinPE` | `bitosdt build-winpe` |
| `Edit-OSDCloudWinPE` | WinPE customization via CLI |
| OSDCloud GUI | Tauri GUI (v2.1) |

## Known Issues

- OSDCloud catalog URLs may have changed - sync service needs update
- USB writing on Windows requires admin privileges
- Some WMI features require Windows 10/11
- Driver catalog sync requires internet connection

## Contributors

- BitOSDT Team

## License

MIT License