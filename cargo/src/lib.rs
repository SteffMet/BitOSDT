//! # BitOSDT 2.0
//!
//! BitOSDT is a comprehensive Windows deployment solution written in Rust.
//! It provides automated Windows deployment with hardware detection, driver
//! management, and WinPE generation.
//!
//! ## Architecture
//!
//! The library is organized into several modules:
//!
//! - **core**: Database, configuration, models, and error handling
//! - **catalog**: OSDCloud driver catalog synchronization and management
//! - **build**: WinPE creation, ISO generation, and USB media writing
//! - **deploy**: Hardware detection, disk operations, and deployment engine
//! - **ui**: Tauri GUI integration (optional)
//! - **utils**: Utilities for logging, downloading, and networking
//!
//! ## Example Usage
//!
//! ```rust,no_run
//! use bitosdt::core::{Config, Database};
//! use bitosdt::deploy::{HardwareDetector, DeploymentEngine};
//!
//! // Initialize configuration
//! let config = Config::load().expect("Failed to load config");
//!
//! // Open database
//! let db = Database::new(&config.database_path).expect("Failed to open database");
//!
//! // Detect hardware
//! let detector = HardwareDetector::new();
//! let hardware = detector.detect_all().expect("Failed to detect hardware");
//!
//! println!("Detected: {} {}", hardware.manufacturer, hardware.model);
//! ```
//!
//! ## Modules
//!
//! ### Core Module
//!
//! Provides fundamental functionality:
//! - **Config**: JSON-based configuration management
//! - **Database**: SQLite database with image and settings storage
//! - **Models**: Data structures for images, hardware, drivers
//! - **Errors**: Comprehensive error handling with thiserror
//!
//! ### Catalog Module
//!
//! Manages driver catalogs:
//! - **CatalogSyncService**: Sync driver catalogs from OSDCloud
//! - **XmlParser**: Parse OSDCloud XML format
//! - **DriverPack**: DriverPack data structures
//! - **OsCatalog**: Built-in Windows OS versions
//!
//! ### Build Module
//!
//! Creates bootable media:
//! - **WinPEBuilder**: Build WinPE with ADK
//! - **IsoCreator**: Create bootable ISO files
//! - **UsbWriter**: Write ISO to USB devices
//!
//! ### Deploy Module
//!
//! Handles deployment operations:
//! - **DeploymentEngine**: Orchestrates full deployment
//! - **HardwareDetector**: Detect system specifications
//! - **DiskManager**: Partition and format disks
//! - **WimManager**: WIM file operations
//! - **BootManager**: Bootloader configuration
//! - **DriverManager**: Driver installation
//!
//! ## Features
//!
//! - `gui`: Enables Tauri-based GUI (requires additional dependencies)
//!
//! ## Platform Support
//!
//! - **Windows**: Full functionality with ADK integration
//! - **Linux**: Development mode with mock implementations
//!
//! ## License
//!
//! MIT License

pub mod build;
pub mod catalog;
pub mod core;
pub mod deploy;
pub mod ui;
pub mod utils;

// BitOSDT 2.0 Image Creation Modules
pub mod config;
pub mod download;
pub mod policy;
pub mod tasks;

pub use core::*;
