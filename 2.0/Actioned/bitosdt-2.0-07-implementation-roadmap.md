# BitOSDT 2.0 - Implementation Roadmap

## Overview

Development phases and timeline for BitOSDT 2.0.

**Revised Timeline: 27 Weeks (~6.5 Months)**

> Note: Original 22-week estimate was aggressive. This revised timeline includes
> realistic buffer for hardware testing, edge cases, and QA.

---

## Key Architectural Decisions

Before implementation begins, the following decisions have been made:

1. **CloudDriver Deferred** - Dynamic Microsoft Update Catalog querying during WinPE
   is deferred to post-deployment. No official API exists, and web scraping is fragile.
   Focus on DriverPack support for v1.0.

2. **Custom JSON Catalog** - Instead of parsing OSDCloud's XML catalogs directly,
   maintain our own JSON catalog format. Sync from OSDCloud periodically with local
   cache fallback for resilience.

3. **VBScript Removed** - VBScript is deprecated in Windows 11 24H2+. All post-deployment
   scripts must be PowerShell. Scripting component excluded from default WinPE.

4. **DISM for WIM Operations** - Use Windows DISM via subprocess rather than pure Rust
   WIM manipulation. Proven, reliable, less development risk.

5. **Code Style** - Rust code follows Rust 2021 edition standards with clippy linting.
   Frontend follows React 18+ with TypeScript.
   Ensure we keep file size to a minimum, modularise the script/code to keep it as small as possible. Ensure folder directory is clean and easy to navigate.
---

## Phase 1: Foundation (Weeks 1-3) 🟢 LOW RISK

### Week 1: Project Setup
- [ ] Create a folder structure within BitOSDT folder, in new folder called cargo, all contents to be stored in this. 
- [ ] Initialize Rust workspace with Cargo
- [ ] Set up Tauri project structure
- [ ] Configure development environment
- [ ] Set up React frontend scaffold
- [ ] Create database module with rusqlite
- [ ] Implement basic error handling

**Deliverables:**
- Working Tauri + React application
- Database schema implemented
- Basic navigation structure

### Week 2: Core Infrastructure
- [ ] Implement configuration management
- [ ] Create logging infrastructure
- [ ] Build progress tracking system
- [ ] Implement network download utilities
- [ ] Create temporary file management
- [ ] Set up Windows API bindings (windows-rs)

**Deliverables:**
- Configuration system
- Download manager with progress
- Logging to file and console

### Week 3: Database and Models
- [ ] Implement all database models
- [ ] Create CRUD operations for images
- [ ] Create CRUD operations for tasks
- [ ] Implement settings management
- [ ] Build audit logging
- [ ] Create data validation

**Deliverables:**
- Full database layer
- Image management UI (basic)
- Settings page

---

## Phase 2: Image Management (Weeks 4-6) 🟢 LOW RISK

### Week 4: Catalog Integration
- [ ] Build catalog sync service (OSDCloud XML → local JSON)
- [ ] Implement catalog fallback (local cache if sync fails)
- [ ] Create OS version selector
- [ ] Implement Windows ESD downloader
- [ ] Create download progress UI
- [ ] Add pause/resume support

**Deliverables:**
- OS catalog browser
- ESD download functionality
- Download queue management
- Resilient catalog system

### Week 5: Image Configuration
- [ ] Build image creation wizard (6-step)
- [ ] Implement OS selection step
- [ ] Create license configuration
- [ ] Build driver preferences UI
- [ ] Implement Autopilot configuration
- [ ] Create task builder interface

**Deliverables:**
- Image creation wizard
- Configuration forms
- Task management UI

### Week 6: Image Builder Integration
- [ ] Integrate windows-iso-downloader code
- [ ] Build ESD to WIM conversion
- [ ] Create image preparation pipeline
- [ ] Implement workspace management
- [ ] Add image validation
- [ ] Build image list view with status

**Deliverables:**
- Working image creation
- Image list with status
- Basic image editing

---

## Phase 3: Hardware Detection (Weeks 7-9) 🟡 MEDIUM RISK

> Extended from 2 weeks to 3 weeks. Hardware edge cases are common.

### Week 7: WMI Integration
- [ ] Implement WMI connection (windows-rs or wmi-rs crate)
- [ ] Create hardware detection module
- [ ] Build manufacturer normalization (Dell, HP, Lenovo, Microsoft)
- [ ] Implement VM detection (VMware, Hyper-V, VirtualBox, etc.)
- [ ] Create form factor detection (laptop, desktop, tablet, server)
- [ ] Build hardware info gathering (CPU, RAM, disk, network)

**Deliverables:**
- Hardware detection library
- Initial testing on development machine

### Week 8: Manufacturer Testing
- [ ] Test on Dell hardware (Latitude, OptiPlex, Precision)
- [ ] Test on HP hardware (EliteBook, ProBook, ProDesk)
- [ ] Test on Lenovo hardware (ThinkPad, ThinkCentre)
- [ ] Test on Microsoft Surface devices
- [ ] Document manufacturer-specific quirks

**Deliverables:**
- Manufacturer compatibility notes
- Edge case fixes

### Week 9: VM and Edge Case Testing
- [ ] Test VM detection (VMware, Hyper-V, VirtualBox, QEMU)
- [ ] Test cloud VMs (Azure, AWS, GCP)
- [ ] Test legacy hardware
- [ ] Test ARM64 devices (if available)
- [ ] Create hardware compatibility matrix
- [ ] Fix remaining edge cases

**Deliverables:**
- Tested hardware detection
- Hardware compatibility matrix document

---

## Phase 4: Driver System (Weeks 10-13) 🟡 MEDIUM RISK

> CloudDriver deferred. Focus on DriverPack which is reliable.

### Week 10: Catalog Sync System
- [ ] Build OSDCloud XML catalog parser
- [ ] Create JSON conversion layer
- [ ] Implement catalog merge into SQLite
- [ ] Build periodic sync scheduler
- [ ] Create sync status UI
- [ ] Implement manual sync trigger

**Deliverables:**
- Catalog sync service
- Local DriverPack database

### Week 11: DriverPack Download
- [ ] Build DriverPack download manager
- [ ] Implement download progress with resume
- [ ] Create extraction for CAB files (expand.exe)
- [ ] Create extraction for ZIP files (zip crate)
- [ ] Create extraction for Dell EXE files (/s /e)
- [ ] Create extraction for HP SoftPaq (7z)
- [ ] Add verification (MD5/SHA256)

**Deliverables:**
- DriverPack download system
- Multi-format extraction

### Week 12: Driver Matching
- [ ] Implement hardware-to-DriverPack matching
- [ ] Build Dell matching (SystemSKUNumber)
- [ ] Build HP matching (BaseBoard.Product)
- [ ] Build Lenovo matching (Model first 4 chars)
- [ ] Build Microsoft Surface matching
- [ ] Implement fallback logic (no match found)

**Deliverables:**
- Automatic DriverPack selection
- Match confidence scoring

### Week 13: Driver Testing
- [ ] Test driver injection on Dell systems
- [ ] Test driver injection on HP systems
- [ ] Test driver injection on Lenovo systems
- [ ] Test offline driver cache scenario
- [ ] Test with no network (embedded drivers)
- [ ] Fix driver compatibility issues

**Deliverables:**
- Working driver system
- Tested on multiple devices

---

## Phase 5: WinPE Builder (Weeks 14-17) 🟡 MEDIUM RISK

> Extended from 3 weeks to 4 weeks. Boot testing is critical.

### Week 14: WinPE Foundation
- [ ] Implement ADK path detection (registry + common paths)
- [ ] Create WinPE base copying
- [ ] Build WIM mounting system (DISM)
- [ ] Implement optional component adding
- [ ] Handle component dependencies
- [ ] Build file copy system

**Deliverables:**
- WinPE image creation
- Component management

### Week 15: BitOSDT Integration
- [ ] Create startup script generation (startnet.cmd)
- [ ] Implement deployment binary embedding
- [ ] Build configuration file embedding
- [ ] Create driver cache embedding (optional)
- [ ] Implement WIM unmount with commit
- [ ] Add WinPE image validation

**Deliverables:**
- Customized WinPE with BitOSDT

### Week 16: Boot Media Creation
- [ ] Create UEFI-bootable ISO generation (oscdimg)
- [ ] Implement USB drive detection
- [ ] Build USB formatting and writing
- [ ] Create hybrid BIOS/UEFI boot support
- [ ] Add boot media verification
- [ ] Build media creation progress UI

**Deliverables:**
- Bootable ISO creation
- USB deployment media

### Week 17: WinPE Testing
- [ ] Test WinPE boot in VMs (VMware, Hyper-V, VirtualBox)
- [ ] Test UEFI boot on physical hardware
- [ ] Test legacy BIOS boot
- [ ] Test on various manufacturers
- [ ] Optimize WinPE size
- [ ] Fix boot issues and driver loading

**Deliverables:**
- Working WinPE images
- Boot compatibility verified

---

## Phase 6: Deployment Engine (Weeks 18-21) 🔴 HIGH RISK

> Extended from 3 weeks to 4 weeks. Disk operations are destructive.
> Extensive testing required before real hardware deployment.

### Week 18: Disk Operations
- [ ] Implement disk detection (WMI Win32_DiskDrive)
- [ ] Build disk selection logic (skip USB, select largest)
- [ ] Create partition clearing (diskpart clean)
- [ ] Implement GPT partition creation
- [ ] Implement MBR partition creation (legacy)
- [ ] Build drive letter assignment

**Deliverables:**
- Disk management library
- Partition operations

### Week 19: WIM Deployment
- [ ] Implement WIM/ESD detection
- [ ] Build WIM application (DISM /Apply-Image)
- [ ] Create progress reporting
- [ ] Implement driver injection to offline image
- [ ] Build error handling and recovery
- [ ] Create WIM validation

**Deliverables:**
- WIM manipulation
- Image deployment

### Week 20: System Configuration
- [ ] Implement BCDBoot configuration (UEFI)
- [ ] Implement BCDBoot configuration (BIOS)
- [ ] Build unattend.xml installation
- [ ] Implement task setup (SetupComplete.cmd)
- [ ] Create Autopilot JSON installation
- [ ] Build post-deployment script injection

**Deliverables:**
- Complete deployment engine
- Task system

### Week 21: Deployment Testing
- [ ] Test full deployment in VMs (multiple times)
- [ ] Test deployment on physical test hardware
- [ ] Test error scenarios (disk failure, WIM corrupt)
- [ ] Test task execution after first boot
- [ ] Test Autopilot enrollment flow
- [ ] Fix critical deployment issues

**Deliverables:**
- Validated deployment engine
- Error handling verified

---

## Phase 7: UI Polish (Weeks 22-23) 🟢 LOW RISK

### Week 22: UI Enhancement
- [ ] Polish all UI components
- [ ] Implement dark/light theme
- [ ] Add animations and transitions
- [ ] Create dashboard view
- [ ] Build device management UI
- [ ] Implement deployment monitoring

**Deliverables:**
- Polished UI
- Theme support

### Week 23: UX Improvements
- [ ] Add keyboard shortcuts
- [ ] Implement context menus
- [ ] Create drag-and-drop support
- [ ] Add import/export functionality
- [ ] Build backup/restore
- [ ] Create user preferences

**Deliverables:**
- Enhanced UX
- Import/export

---

## Phase 8: Testing and Documentation (Weeks 24-26) 🟡 MEDIUM RISK

> Extended from 2 weeks to 3 weeks. Deployment tools need thorough QA.

### Week 24: Automated Testing
- [ ] Write unit tests for all modules
- [ ] Create integration tests for workflows
- [ ] Build test fixtures (mock WMI data)
- [ ] Implement CI/CD pipeline
- [ ] Add code coverage reporting

**Deliverables:**
- Unit test suite (>80% coverage)
- CI/CD pipeline

### Week 25: Hardware and End-to-End Testing
- [ ] Full deployment test on Dell hardware
- [ ] Full deployment test on HP hardware
- [ ] Full deployment test on Lenovo hardware
- [ ] Full deployment test on VMs
- [ ] Test various Windows versions (10, 11, Server)
- [ ] Performance testing and optimization

**Deliverables:**
- Hardware test reports
- Performance benchmarks

### Week 26: Documentation
- [ ] Write user documentation
- [ ] Create API documentation
- [ ] Build troubleshooting guide
- [ ] Create quick-start video
- [ ] Write developer docs
- [ ] Document known issues and workarounds

**Deliverables:**
- Complete documentation
- Quick-start guide

---

## Phase 9: Release Preparation (Week 27) 🟢 LOW RISK

- [ ] Create installer (NSIS or WiX)
- [ ] Build release pipeline
- [ ] Code signing setup (if certificate available)
- [ ] Create release notes
- [ ] Prepare GitHub release
- [ ] Beta tester outreach

**Deliverables:**
- Release-ready build
- Installer
- Release notes

---

## Total Timeline: 27 Weeks (~6.5 Months)

| Phase | Weeks | Risk Level |
|-------|-------|------------|
| Foundation | 1-3 | 🟢 Low |
| Image Management | 4-6 | 🟢 Low |
| Hardware Detection | 7-9 | 🟡 Medium |
| Driver System | 10-13 | 🟡 Medium |
| WinPE Builder | 14-17 | 🟡 Medium |
| Deployment Engine | 18-21 | 🔴 High |
| UI Polish | 22-23 | 🟢 Low |
| Testing & Docs | 24-26 | 🟡 Medium |
| Release Prep | 27 | 🟢 Low |

---

## Milestones

### Milestone 1: Foundation Complete (Week 3)
- Working application shell
- Database operational
- Basic UI navigation

### Milestone 2: Image Management Complete (Week 6)
- Image creation wizard working
- OS catalog integration with sync
- Download management with resume

### Milestone 3: Hardware & Drivers Complete (Week 13)
- Hardware detection working on Dell, HP, Lenovo, Surface
- DriverPack system operational
- Tested on multiple physical devices

### Milestone 4: WinPE Complete (Week 17)
- WinPE creation working
- Bootable ISO and USB generation
- Tested boot process on VMs and hardware

### Milestone 5: Deployment Complete (Week 21)
- Full deployment engine operational
- End-to-end deployment tested
- Task and Autopilot systems working

### Milestone 6: Beta Release (Week 27)
- Production-ready build
- Complete documentation
- Public beta release

---

## Risk Mitigation

### High-Risk Areas

1. **Disk Operations** 🔴
   - Risk: Data loss from bugs in partition/format code
   - Mitigation: Extensive VM testing, confirmation dialogs, "dry-run" mode
   - Fallback: Use diskpart scripts if custom code fails

2. **WinPE Boot Compatibility** 🟡
   - Risk: Boot failures on certain hardware
   - Mitigation: Test on multiple manufacturers, include generic drivers
   - Fallback: Document unsupported hardware, provide manual driver injection

3. **Driver Matching Accuracy** 🟡
   - Risk: Wrong DriverPack selected, deployment fails
   - Mitigation: Confidence scoring, fallback to no drivers (Windows will find)
   - Fallback: Manual DriverPack selection in UI

4. **OSDCloud Catalog Availability** 🟡
   - Risk: OSDCloud repository goes offline or changes format
   - Mitigation: Local cache, periodic snapshots, custom catalog as fallback
   - Fallback: Manual catalog entry creation

### Deferred/Removed Risks

1. **CloudDriver (Microsoft Update)** - Deferred to post-deployment phase
   - No fragile web scraping in critical boot path
   - Can be added as v1.1 feature

2. **VBScript Support** - Removed
   - Deprecated in Windows 11 24H2+
   - PowerShell-only reduces complexity

### Contingency Plans

- **Schedule slips by 2+ weeks:** Cut themes, advanced task builder
- **Hardware testing reveals major issues:** Limit v1.0 to tested manufacturers
- **Deployment engine unstable:** Release as "image builder only" first

---

## Development Priorities

### Must Have (MVP)
1. Image creation wizard
2. WinPE generation (ISO + USB)
3. Hardware detection (Dell, HP, Lenovo)
4. DriverPack support with catalog sync
5. Basic deployment engine
6. Basic task system (PowerShell scripts)

### Should Have (v1.0)
1. Autopilot integration
2. Device tracking/history
3. Dark/light theme
4. Import/export configurations
5. Comprehensive testing

### Nice to Have (v1.1+)
1. CloudDriver (post-deployment)
2. PXE boot
3. Advanced task builder
4. Linux build server support
5. Plugin system
6. Cloud sync

---

## Development Workflow

### Git Workflow
- `main` branch: Production-ready
- `develop` branch: Integration
- Feature branches: `feature/xxx`
- Hotfix branches: `hotfix/xxx`

### Code Reviews
- All changes via pull request
- Minimum 1 approval required
- CI/CD runs on all PRs
- No merges with failing tests

### Testing Strategy
- Unit tests for all modules (>80% coverage target)
- Integration tests for workflows
- VM testing for every deployment change
- Physical hardware testing before milestones
- Regression testing via CI/CD

### Documentation
- Code documentation in source (rustdoc)
- User docs in /docs folder (Markdown)
- Architecture Decision Records (ADRs) for major decisions

---

## Success Metrics

1. **Functionality**: Feature parity with OSDCloud core features
2. **Performance**: Image creation < 10 min, deployment < 20 min
3. **Reliability**: < 2% failure rate on supported hardware
4. **Compatibility**: Support Dell, HP, Lenovo, Microsoft Surface
5. **User Satisfaction**: Positive feedback from beta testers
6. **Code Quality**: >80% test coverage, no critical bugs at release
