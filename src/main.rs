//! RustyNES - A cycle-accurate Nintendo Entertainment System emulator
//!
//! This is the main entry point for the emulator. It handles command-line arguments,
//! initializes the system, loads ROMs, and runs the main emulation loop.

use anyhow::{Context, Result};
use clap::Parser;
use log::{error, info};
use std::path::PathBuf;

mod apu;
mod audio;
mod cartridge;
mod cpu;
mod mappers;
mod memory;
mod nes;
mod ppu;
mod controller;
// mod savestate;
mod util;

use nes::NES;
use ppu::TVSystem;

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
    
    /// Use PAL TV system instead of NTSC
    #[clap(long)]
    pal: bool,
    
    /// Scale factor for display (default: 3)
    #[clap(short, long, default_value = "3")]
    scale: u32,
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
    
    // Determine TV system
    let tv_system = if args.pal {
        TVSystem::PAL
    } else {
        TVSystem::NTSC
    };
    
    // Create and initialize the NES
    let mut nes = NES::new(tv_system, args.scale);
    
    // Load the ROM file
    let rom_path = args.rom_path.to_string_lossy();
    info!("Loading ROM: {}", rom_path);
    
    nes.load_cartridge_from_file(&args.rom_path)
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