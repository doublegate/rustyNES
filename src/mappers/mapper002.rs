//! Mapper 002 (UxROM) implementation
//!
//! This mapper features PRG ROM banking with fixed last bank.
//! Used by games like Mega Man, Duck Tales, Castlevania, etc.
//!
//! Memory map:
//! - PRG ROM: Switchable 16KB bank + fixed 16KB bank
//! - CHR ROM/RAM: 8KB (fixed)

use crate::cartridge::{Mirroring, CartridgeTrait};
use super::Mapper;

pub struct Mapper002 {
    /// PRG ROM data
    prg_rom: Vec<u8>,
    
    /// CHR ROM/RAM data
    chr: Vec<u8>,
    
    /// Whether CHR is RAM (writable) or ROM (read-only)
    chr_is_ram: bool,
    
    /// Current PRG ROM bank
    prg_bank: u8,
    
    /// Mirroring mode
    mirroring: Mirroring,
}

impl Mapper002 {
    /// Create a new Mapper002 instance
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, chr_ram_size: usize, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_rom.is_empty();
        let chr = if chr_is_ram {
            vec![0; chr_ram_size]
        } else {
            chr_rom
        };
        
        Mapper002 {
            prg_rom,
            chr,
            chr_is_ram,
            prg_bank: 0,
            mirroring,
        }
    }
}

impl Mapper for Mapper002 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xBFFF => {
                // Switchable bank
                let bank_size = 16 * 1024;
                let bank_count = self.prg_rom.len() / bank_size;
                let bank = self.prg_bank % bank_count as u8;
                let offset = (addr & 0x3FFF) as usize;
                self.prg_rom[(bank as usize * bank_size) + offset]
            },
            0xC000..=0xFFFF => {
                // Fixed last bank
                let bank_size = 16 * 1024;
                let bank_count = self.prg_rom.len() / bank_size;
                let last_bank = bank_count - 1;
                let offset = (addr & 0x3FFF) as usize;
                self.prg_rom[(last_bank * bank_size) + offset]
            },
            _ => 0,
        }
    }
    
    fn write_prg(&mut self, addr: u16, data: u8) {
        if addr >= 0x8000 {
            // Bank select (ignore address, only data matters)
            self.prg_bank = data & 0x0F;
        }
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
        // No IRQ in UxROM
    }
    
    fn notify_scanline(&mut self) {
        // No scanline counter in UxROM
    }
    
    fn reset(&mut self) {
        self.prg_bank = 0;
    }
}

impl CartridgeTrait for Mapper002 {
    fn load_ram(&mut self, _data: &[u8]) {
        // UxROM has no PRG RAM
    }
}