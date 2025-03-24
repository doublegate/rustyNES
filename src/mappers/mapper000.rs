//! Mapper 000 (NROM) implementation
//!
//! This is the simplest NES mapper with no banking capabilities.
//! Used by games like Super Mario Bros, Donkey Kong, etc.
//!
//! Memory map:
//! - PRG ROM: 16KB (0x8000-0xBFFF) or 32KB (0x8000-0xFFFF)
//! - CHR ROM/RAM: 8KB (0x0000-0x1FFF)

use crate::cartridge::{Mirroring, CartridgeTrait};
use super::Mapper;

pub struct Mapper000 {
    /// PRG ROM data
    prg_rom: Vec<u8>,
    
    /// CHR ROM/RAM data
    chr: Vec<u8>,
    
    /// Whether CHR is RAM (writable) or ROM (read-only)
    chr_is_ram: bool,
    
    /// Mirroring mode
    mirroring: Mirroring,
}

impl Mapper000 {
    /// Create a new Mapper000 instance
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, chr_ram_size: usize, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_rom.is_empty();
        let chr = if chr_is_ram {
            vec![0; chr_ram_size]
        } else {
            chr_rom
        };
        
        Mapper000 {
            prg_rom,
            chr,
            chr_is_ram,
            mirroring,
        }
    }
}

impl Mapper for Mapper000 {
    fn read_prg(&self, addr: u16) -> u8 {
        let mask = if self.prg_rom.len() <= 16 * 1024 { 0x3FFF } else { 0x7FFF };
        self.prg_rom[(addr & mask) as usize]
    }
    
    fn write_prg(&mut self, _addr: u16, _data: u8) {
        // PRG ROM is read-only in NROM
    }
    
    fn read_chr(&self, addr: u16) -> u8 {
        self.chr[(addr & 0x1FFF) as usize]
    }
    
    fn write_chr(&mut self, addr: u16, data: u8) {
        if self.chr_is_ram {
            self.chr[(addr & 0x1FFF) as usize] = data;
        }
    }
    
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    
    fn irq_triggered(&self) -> bool {
        false
    }
    
    fn acknowledge_irq(&mut self) {
        // No IRQ in NROM
    }
    
    fn notify_scanline(&mut self) {
        // No scanline counter in NROM
    }
    
    fn reset(&mut self) {
        // Nothing to reset in NROM
    }
}

impl CartridgeTrait for Mapper000 {
    fn load_ram(&mut self, _data: &[u8]) {
        // NROM has no PRG RAM
    }
}