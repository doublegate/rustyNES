//! RustyNES - A cycle-accurate Nintendo Entertainment System emulator
//!
//! This is the main entry point for the emulator. It handles command-line arguments,
//! initializes the system, loads ROMs, and runs the main emulation loop.

use anyhow::{Context, Result};
use clap::Parser;
use log::{error, info};
use std::path::PathBuf;

mod apu;
mod cartridge;
mod cpu;
mod memory;
mod nes;
mod ppu;
mod controller;
// mod util;

/// Command line arguments for RustyNES
#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    /// Path to the NES ROM file
    #[clap(name = "ROM")]
    rom_path: PathBuf,

    /// Enable debug logging
    #[clap(short, long)]
    debug: bool,
}

fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();

    // Initialize logging
    if args.debug {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    }

    info!("RustyNES emulator starting...");
    
    // Load the ROM
    let rom_path = args.rom_path.to_string_lossy();
    info!("Loading ROM: {}", rom_path);
    
    // Load the ROM file
    let rom_data = std::fs::read(&args.rom_path)
        .with_context(|| format!("Failed to read ROM file: {}", rom_path))?;
    
    // Create and initialize the NES
    let mut nes = nes::NES::new();
    nes.load_cartridge(&rom_data)
        .with_context(|| format!("Failed to load ROM: {}", rom_path))?;
    
    // Run the emulator
    match nes.run() {
        Ok(_) => {
            info!("Emulation completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Emulation error: {}", e);
            Err(e)
        }
    }
}