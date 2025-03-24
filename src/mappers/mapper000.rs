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
    
    /// PRG ROM mask for fast address calculation
    prg_mask: u16,
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
        
        // Calculate PRG mask for fast address mapping
        // 16KB PRG ROM gets mirrored to fill the 32KB space
        let prg_mask = if prg_rom.len() <= 16 * 1024 { 0x3FFF } else { 0x7FFF };
        
        Mapper000 {
            prg_rom,
            chr,
            chr_is_ram,
            mirroring,
            prg_mask,
        }
    }
}

impl Mapper for Mapper000 {
    #[inline]
    fn read_prg(&self, addr: u16) -> u8 {
        // Fast address calculation using pre-computed mask
        let mapped_addr = (addr & self.prg_mask) as usize;
        if mapped_addr < self.prg_rom.len() {
            self.prg_rom[mapped_addr]
        } else {
            0  // Return 0 for out-of-bounds access
        }
    }
    
    #[inline]
    fn write_prg(&mut self, _addr: u16, _data: u8) {
        // PRG ROM is read-only in NROM
    }
    
    #[inline]
    fn read_chr(&self, addr: u16) -> u8 {
        let mapped_addr = (addr & 0x1FFF) as usize;
        if mapped_addr < self.chr.len() {
            self.chr[mapped_addr]
        } else {
            0  // Return 0 for out-of-bounds access
        }
    }
    
    #[inline]
    fn write_chr(&mut self, addr: u16, data: u8) {
        if self.chr_is_ram {
            let mapped_addr = (addr & 0x1FFF) as usize;
            if mapped_addr < self.chr.len() {
                self.chr[mapped_addr] = data;
            }
        }
    }
    
    #[inline]
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    
    #[inline]
    fn irq_triggered(&self) -> bool {
        false
    }
    
    #[inline]
    fn acknowledge_irq(&mut self) {
        // No IRQ in NROM
    }
    
    #[inline]
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