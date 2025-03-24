//! Mapper 003 (CNROM) implementation
//!
//! This mapper features CHR ROM banking with fixed PRG ROM.
//! Used by games like Adventure Island, Paperboy, Defender II, etc.
//!
//! Memory map:
//! - PRG ROM: 16KB/32KB (fixed)
//! - CHR ROM: 8KB with banking

use crate::cartridge::{Mirroring, CartridgeTrait};
use super::Mapper;

pub struct Mapper003 {
    /// PRG ROM data
    prg_rom: Vec<u8>,
    
    /// CHR ROM/RAM data
    chr: Vec<u8>,
    
    /// Whether CHR is RAM (writable) or ROM (read-only)
    chr_is_ram: bool,
    
    /// Current CHR ROM bank
    chr_bank: u8,
    
    /// Mirroring mode
    mirroring: Mirroring,
}

impl Mapper003 {
    /// Create a new Mapper003 instance
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, chr_ram_size: usize, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_rom.is_empty();
        let chr = if chr_is_ram {
            vec![0; chr_ram_size]
        } else {
            chr_rom
        };
        
        Mapper003 {
            prg_rom,
            chr,
            chr_is_ram,
            chr_bank: 0,
            mirroring,
        }
    }
}

impl Mapper for Mapper003 {
    fn read_prg(&self, addr: u16) -> u8 {
        let mask = if self.prg_rom.len() <= 16 * 1024 { 0x3FFF } else { 0x7FFF };
        self.prg_rom[(addr & mask) as usize]
    }
    
    fn write_prg(&mut self, addr: u16, data: u8) {
        if addr >= 0x8000 {
            // CHR bank select (ignore address, only data matters)
            self.chr_bank = data & 0x03;
        }
    }
    
    fn read_chr(&self, addr: u16) -> u8 {
        let bank_size = 8 * 1024;
        let bank_count = self.chr.len() / bank_size;
        if bank_count == 0 {
            return 0;
        }
        
        let bank = self.chr_bank % bank_count as u8;
        let offset = (addr & 0x1FFF) as usize;
        self.chr[(bank as usize * bank_size) + offset]
    }
    
    fn write_chr(&mut self, addr: u16, data: u8) {
        if self.chr_is_ram {
            let bank_size = 8 * 1024;
            let bank = self.chr_bank as usize;
            let offset = (addr & 0x1FFF) as usize;
            let chr_addr = (bank * bank_size) + offset;
            if chr_addr < self.chr.len() {
                self.chr[chr_addr] = data;
            }
        }
    }
    
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    
    fn irq_triggered(&self) -> bool {
        false
    }
    
    fn acknowledge_irq(&mut self) {
        // No IRQ in CNROM
    }
    
    fn notify_scanline(&mut self) {
        // No scanline counter in CNROM
    }
    
    fn reset(&mut self) {
        self.chr_bank = 0;
    }
}

impl CartridgeTrait for Mapper003 {
    fn load_ram(&mut self, _data: &[u8]) {
        // CNROM has no PRG RAM
    }
}