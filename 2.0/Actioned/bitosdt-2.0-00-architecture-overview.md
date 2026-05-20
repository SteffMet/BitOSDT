# BitOSDT 2.0 - Architecture Overview

## Project Vision

BitOSDT 2.0 is a complete Rust-based upgrade for BitOSDT (1.0) and a replacement for OSDCloud, providing a modern, open-source Windows deployment solution with enhanced driver management and task automation.

## Core Principles

1. **Full OSDCloud Replacement** - Complete feature parity with OSDCloud
2. **WinPE-Based Deployment** - Uses Windows Preinstallation Environment
3. **Cross-Platform Build** - Windows-first, Linux-compatible code
4. **No PowerShell Dependencies** - Pure Rust deployment engine
5. **Modular Architecture** - Easy to extend and maintain

---

## Key Architectural Decisions

The following decisions were made to ensure feasibility and reliability:

### 1. CloudDriver Deferred to Post-Deployment

**Decision:** CloudDriver (Microsoft Update Catalog queries) is deferred to v1.1.

**Rationale:**
- No official REST API exists for the Microsoft Update Catalog
- Web scraping during WinPE boot is fragile and unreliable
- Network dependency in critical boot path is a risk

**v1.0 Approach:**
- Focus on DriverPack support (manufacturer driver packages)
- DriverPacks are reliable, well-documented, and cacheable
- CloudDriver can be added post-deployment in full Windows environment

### 2. Custom JSON Catalog Format

**Decision:** Maintain our own JSON catalog format, synced from OSDCloud.

**Rationale:**
- OSDCloud uses XML format, not JSON
- XML parsing is more complex and error-prone
- JSON is native to JavaScript frontend and easier to work with in Rust

**Implementation:**
- Catalog Sync Service converts OSDCloud XML → local JSON
- JSON catalog stored in SQLite for fast queries
- Offline fallback if sync fails

### 3. VBScript Removed from WinPE

**Decision:** VBScript support is excluded from default WinPE components.

**Rationale:**
- VBScript is deprecated in Windows 11 24H2+
- Microsoft is removing VBScript from future Windows versions
- PowerShell provides all needed scripting capabilities

**Impact:**
- Task system uses PowerShell exclusively
- Scripting component removed from default WinPE build
- StorageWmi component added for storage management

### 4. DISM for WIM Operations

**Decision:** Use Windows DISM via subprocess rather than pure Rust WIM manipulation.

**Rationale:**
- DISM is proven and reliable
- Pure Rust WIM libraries are immature
- Less development risk and maintenance burden
- Consistent behavior with Windows tools

**Implementation:**
- Subprocess calls to `dism.exe` with proper argument formatting
- Error parsing from DISM output
- Progress extraction where possible

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        BitOSDT 2.0                                   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌─────────────────────┐        ┌─────────────────────────────────┐ │
│  │   Build System      │        │   Deployment Runtime (WinPE)    │ │
│  │   (Host OS)         │        │   (Target Device)               │ │
│  └─────────────────────┘        └─────────────────────────────────┘ │
│           │                                    │                     │
│           ▼                                    ▼                     │
│  ┌─────────────────────┐        ┌─────────────────────────────────┐ │
│  │ • Tauri + React UI  │        │ • WinPE Boot Environment        │ │
│  │ • Image Management  │        │ • Rust Deployment Binary        │ │
│  │ • WinPE Builder     │        │ • Hardware Detection            │ │
│  │ • Catalog Fetcher   │        │ • Driver Download/Install       │ │
│  │ • ISO/USB Creation  │        │ • Disk Operations               │ │
│  │ • Task Builder      │        │ • WIM Application               │ │
│  └─────────────────────┘        │ • Bootloader Config             │ │
│                                 │ • Post-Deploy Tasks             │ │
│                                 └─────────────────────────────────┘ │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Technology Stack

### Core Language & Runtime
- **Rust** - Primary language
- **Tokio** - Async runtime
- **Windows-rs** - Windows API bindings

### Build System
- **Tauri** - Desktop UI framework
- **React** - Frontend UI
- **SQLite** - Local database (rusqlite)

### WinPE Runtime
- **WinPE** - Boot environment (from ADK)
- **Rust binary** - Deployment engine
- **Windows APIs** - Native system integration

### External Dependencies
- **Windows ADK** - WinPE creation tools
- **wimlib** - WIM manipulation (optional, for Linux compatibility)
- **OSDCloud Catalogs** - Driver and OS catalogs

## Module Structure

```
src/
├── main.rs                    # Application entry point
├── lib.rs                     # Library exports
│
├── core/                      # Core infrastructure
│   ├── mod.rs
│   ├── config.rs             # Configuration management
│   ├── database.rs           # SQLite database operations
│   ├── models.rs             # Data structures
│   └── errors.rs             # Error handling
│
├── catalog/                   # Catalog management
│   ├── mod.rs
│   ├── sync_service.rs       # OSDCloud XML → JSON sync
│   ├── xml_parser.rs         # OSDCloud XML catalog parser
│   ├── driverpack.rs         # DriverPack catalog (JSON)
│   ├── matcher.rs            # Hardware-to-driver matching
│   └── cache.rs              # Local catalog caching + fallback
│
├── build/                     # Build-time operations
│   ├── mod.rs
│   ├── winpe_builder.rs      # WinPE image creation
│   ├── iso_creator.rs        # ISO generation
│   ├── usb_writer.rs         # USB creation
│   └── image_preparer.rs     # Image customization
│
├── deploy/                    # Deployment engine (WinPE runtime)
│   ├── mod.rs
│   ├── engine.rs             # Main deployment orchestrator
│   ├── hardware.rs           # Hardware detection
│   ├── drivers.rs            # Driver management
│   ├── disk.rs               # Disk operations
│   ├── wim.rs                # WIM/ESD operations
│   ├── boot.rs               # Bootloader configuration
│   └── tasks.rs              # Post-deployment tasks
│
├── ui/                        # User interface
│   ├── mod.rs
│   ├── commands.rs           # Tauri commands
│   └── state.rs              # UI state management
│
└── utils/                     # Utilities
    ├── mod.rs
    ├── logging.rs            # Logging infrastructure
    ├── progress.rs           # Progress reporting
    └── net.rs                # Network utilities
```

## Data Flow

### Image Creation Flow

```
1. User selects OS version (Windows 10/11)
   ↓
2. App downloads ESD from Microsoft CDN
   ↓
3. User configures deployment options:
   - Autopilot settings
   - Post-deployment tasks
   - Driver preferences
   ↓
4. App creates WinPE image:
   - Base WinPE from ADK
   - Embedded Rust deployment binary
   - Configuration files
   ↓
5. App creates bootable media:
   - ISO file for PXE/VM
   - USB drive for physical deployment
```

### Deployment Flow

```
1. Target device boots from WinPE media
   ↓
2. Rust deployment binary starts
   ↓
3. Hardware detection:
   - Query WMI for manufacturer, model, product
   - Detect architecture and form factor
   ↓
4. Driver acquisition:
   - Query embedded JSON catalog for matching DriverPack
   - Download manufacturer DriverPack (if network available)
   - Fall back to pre-cached drivers (if embedded in WinPE)
   ↓
5. Disk preparation:
   - Clear disk
   - Create partitions (EFI, MSR, Windows, Recovery)
   ↓
6. Image deployment:
   - Extract Windows WIM from ESD
   - Apply to disk
   - Inject drivers
   ↓
7. System configuration:
   - Configure bootloader (BCDBoot)
   - Install unattend.xml
   - Configure post-deployment tasks
   ↓
8. Reboot to Windows
```

## Key Features

### 1. Hardware Detection
- Automatic manufacturer/model detection
- Product SKU identification
- VM detection
- Form factor detection (laptop/desktop/server)

### 2. Driver Management
- **DriverPack**: Download manufacturer driver packs (Dell, HP, Lenovo, Microsoft)
  - Catalog synced from OSDCloud (XML → JSON conversion)
  - Automatic hardware-to-driver matching
  - Offline cache support for air-gapped deployments
- **CloudDriver** (v1.1): Microsoft Update Catalog integration post-deployment
- Driver injection into offline Windows image via DISM

### 3. Image Customization
- Autopilot configuration
- Unattend.xml generation
- Post-deployment task configuration
- Software package injection

### 4. Deployment Methods
- USB boot media
- ISO files (for VMs/PXE)
- (Future: PXE network boot)

### 5. Task System
- Install applications
- Run scripts (PowerShell only - VBScript deprecated)
- Copy files
- Domain join
- Device renaming
- Registry modifications

## External Integrations

### OSDCloud Catalogs
- OS catalog (Windows versions)
- DriverPack catalog (manufacturer drivers)
- Sync service: OSDCloud XML → local JSON conversion
- Local caching with TTL + offline fallback

### Microsoft Services
- Windows CDN (ESD downloads)
- Autopilot service
- Windows Update Catalog (v1.1 - CloudDriver post-deployment)

### Manufacturer Services
- Dell Driver Catalog API
- HP SoftPaq FTP
- Lenovo System Update
- Microsoft Surface drivers

## Cross-Platform Compatibility

### Windows Build Server
- Full feature support
- Native ADK integration
- DISM for WIM operations
- Native DriverPack extraction

### Linux Build Server (Future)
- Use wimlib for WIM operations
- HTTP downloads (no Windows-specific APIs)
- Limited driver catalog access

### Code Portability
- Abstract platform-specific operations
- Feature flags for OS-specific code
- Trait-based interfaces for pluggable implementations

## Dependencies

### Required
- Windows ADK (for WinPE)
- Windows 10/11 (for build system)
- Internet connection (for downloads)

### Optional
- wimlib (for advanced WIM operations)
- 7-Zip (for archive extraction)

## Success Criteria

1. **Feature Parity** - Match or exceed OSDCloud functionality
2. **Performance** - Faster image creation and deployment
3. **Reliability** - Robust error handling and recovery
4. **Maintainability** - Clean, well-documented Rust code
5. **Extensibility** - Easy to add new drivers, tasks, features

## Next Steps

See individual plan documents:
- `01-hardware-detection.md` - Hardware detection specification
- `02-driver-system.md` - Driver management architecture
- `03-winpe-builder.md` - WinPE creation process
- `04-deployment-engine.md` - Deployment orchestration
- `05-database-schema.md` - Data models and storage
- `06-ui-specification.md` - User interface design
- `07-implementation-roadmap.md` - Development phases and timeline

