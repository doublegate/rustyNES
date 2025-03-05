//! # rustyNES
//! 
//! A cycle-accurate Nintendo Entertainment System (NES) emulator written in Rust.
//! 
//! This is the main entry point for the emulator.

mod nes_cpu;

use std::env;
use std::fs::File;
use std::io::Read;
use std::path::Path;

fn main() {
    // Simple command line argument parsing
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        println!("Usage: {} <rom_path>", args[0]);
        return;
    }
    
    let rom_path = &args[1];
    
    // Print welcome message
    println!("rustyNES v0.1.0 - A Nintendo Entertainment System Emulator");
    println!("Loading ROM: {}", rom_path);
    
    // TODO: Load the ROM file
    if let Err(e) = load_rom(rom_path) {
        println!("Error loading ROM: {}", e);
        return;
    }
    
    println!("Successfully loaded ROM. Emulation not yet implemented.");
    println!("Stay tuned for future updates!");
    
    // TODO: Initialize the NES and start emulation
}

fn load_rom(path: &str) -> Result<(), String> {
    let path = Path::new(path);
    
    // Check if the file exists
    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }
    
    // Check if the file has a .nes extension
    if path.extension().unwrap_or_default() != "nes" {
        return Err("File must have a .nes extension".to_string());
    }
    
    // Try to open the file
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) => return Err(format!("Failed to open file: {}", e)),
    };
    
    // Read the header (16 bytes)
    let mut header = [0u8; 16];
    if let Err(e) = file.read_exact(&mut header) {
        return Err(format!("Failed to read NES header: {}", e));
    }
    
    // Check the NES header magic number (NES<EOF>)
    if header[0] != 0x4E || header[1] != 0x45 || header[2] != 0x53 || header[3] != 0x1A {
        return Err("Not a valid NES ROM file (invalid header)".to_string());
    }
    
    // Extract basic information from the header
    let prg_rom_size = header[4] as usize * 16384; // 16KB units
    let chr_rom_size = header[5] as usize * 8192;  // 8KB units
    let mapper = (header[6] >> 4) | (header[7] & 0xF0);
    
    println!("ROM Information:");
    println!("  PRG ROM: {} KB", prg_rom_size / 1024);
    println!("  CHR ROM: {} KB", chr_rom_size / 1024);
    println!("  Mapper: {}", mapper);
    
    // TODO: Load the PRG ROM and CHR ROM data
    
    Ok(())
}
