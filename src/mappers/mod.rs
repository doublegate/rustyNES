//! Mapper implementations for NES cartridges
//!
//! The NES uses various memory mappers to expand the capabilities of the hardware.
//! This module provides implementations for mappers 000-004, which cover a large
//! percentage of the NES game library.

mod mapper000; // NROM
mod mapper001; // MMC1
mod mapper002; // UxROM
mod mapper003; // CNROM
mod mapper004; // MMC3

pub use mapper000::Mapper000;
pub use mapper001::Mapper001;
pub use mapper002::Mapper002;
pub use mapper003::Mapper003;
pub use mapper004::Mapper004;

use crate::cartridge::{Mirroring, CartridgeTrait};

/// Trait for NES mappers
pub trait Mapper: CartridgeTrait {
    /// Read from PRG ROM/RAM
    fn read_prg(&self, addr: u16) -> u8;
    
    /// Write to PRG ROM/RAM
    fn write_prg(&mut self, addr: u16, value: u8);
    
    /// Read from CHR ROM/RAM
    fn read_chr(&self, addr: u16) -> u8;
    
    /// Write to CHR ROM/RAM
    fn write_chr(&mut self, addr: u16, value: u8);
    
    /// Get the current mirroring mode
    fn mirroring(&self) -> Mirroring;
    
    /// Check if an IRQ has been triggered
    fn irq_triggered(&self) -> bool {
        false
    }
    
    /// Acknowledge an IRQ
    fn acknowledge_irq(&mut self) {}
    
    /// Notify that a scanline has been completed
    fn notify_scanline(&mut self) {}

    /// Reset the mapper to its initial state
    fn reset(&mut self);
}

/// Create a new mapper instance based on mapper number
pub fn create_mapper(
    mapper_number: u8,
    prg_rom: Vec<u8>,
    chr_rom: Vec<u8>,
    prg_ram: Vec<u8>,
    chr_ram_size: usize,
    mirroring: Mirroring,
) -> Box<dyn Mapper> {
    match mapper_number {
        0 => Box::new(Mapper000::new(prg_rom, chr_rom, chr_ram_size, mirroring)),
        1 => Box::new(Mapper001::new(prg_rom, chr_rom, prg_ram, mirroring)),
        2 => Box::new(Mapper002::new(prg_rom, chr_rom, chr_ram_size, mirroring)),
        3 => Box::new(Mapper003::new(prg_rom, chr_rom, chr_ram_size, mirroring)),
        4 => Box::new(Mapper004::new(prg_rom, chr_rom, prg_ram, mirroring)),
        _ => panic!("Unsupported mapper: {}", mapper_number),
    }
}