//! Mapper 001 (MMC1) implementation
//!
//! This mapper features PRG ROM banking, CHR ROM banking, and configurable mirroring.
//! Used by games like The Legend of Zelda, Metroid, Final Fantasy, etc.
//!
//! Memory map:
//! - PRG ROM: 16KB/32KB with banking
//! - PRG RAM: 8KB (0x6000-0x7FFF)
//! - CHR ROM/RAM: 8KB with banking

use log::debug;
use crate::cartridge::{Mirroring, CartridgeTrait};
use super::Mapper;

pub struct Mapper001 {
    /// PRG ROM data
    prg_rom: Vec<u8>,
    
    /// CHR ROM/RAM data
    chr: Vec<u8>,
    
    /// PRG RAM data
    prg_ram: Vec<u8>,
    
    /// Whether CHR is RAM (writable) or ROM (read-only)
    chr_is_ram: bool,
    
    /// Shift register for serial MMC1 writes
    shift_register: u8,
    
    /// Shift register bit counter
    shift_count: u8,
    
    /// Control register (0x8000-0x9FFF)
    /// - Bits 0-1: Mirroring
    /// - Bit 2: PRG ROM bank mode
    /// - Bits 3-4: CHR ROM bank mode
    control: u8,
    
    /// CHR bank 0 register (0xA000-0xBFFF)
    chr_bank_0: u8,
    
    /// CHR bank 1 register (0xC000-0xDFFF)
    chr_bank_1: u8,
    
    /// PRG bank register (0xE000-0xFFFF)
    prg_bank: u8,
    
    /// Mirroring mode
    mirroring: Mirroring,
}

impl Mapper001 {
    /// Create a new Mapper001 instance
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, prg_ram: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_rom.is_empty();
        let chr = if chr_is_ram {
            vec![0; 8 * 1024] // 8KB CHR RAM
        } else {
            chr_rom
        };
        
        Mapper001 {
            prg_rom,
            chr,
            prg_ram,
            chr_is_ram,
            shift_register: 0x10, // Reset state
            shift_count: 0,
            control: 0x0C,       // Initial control value
            chr_bank_0: 0,
            chr_bank_1: 0,
            prg_bank: 0,
            mirroring,
        }
    }
    
    /// Get the PRG ROM bank size
    fn prg_bank_size(&self) -> usize {
        16 * 1024 // 16KB banks
    }
    
    /// Get the CHR bank size
    fn chr_bank_size(&self) -> usize {
        4 * 1024 // 4KB banks
    }
    
    /// Get the address for PRG ROM access
    fn prg_addr(&self, addr: u16) -> usize {
        let prg_bank_count = self.prg_rom.len() / self.prg_bank_size();
        
        match (self.control >> 2) & 0x03 {
            0 | 1 => {
                // 32KB mode (ignore bit 0)
                let bank = (self.prg_bank & 0x0E) % (prg_bank_count as u8 & 0xFE);
                let bank_offset = ((addr & 0x7FFF) + (bank as u16 * 0x8000)) as usize;
                bank_offset % self.prg_rom.len()
            },
            2 => {
                // Fixed first bank, switchable second bank
                if addr < 0xC000 {
                    (addr & 0x3FFF) as usize
                } else {
                    let bank = self.prg_bank % prg_bank_count as u8;
                    let bank_offset = ((addr & 0x3FFF) + (bank as u16 * 0x4000)) as usize;
                    bank_offset % self.prg_rom.len()
                }
            },
            3 => {
                // Switchable first bank, fixed last bank
                if addr >= 0xC000 {
                    let bank = (prg_bank_count - 1) as u8;
                    let bank_offset = ((addr & 0x3FFF) + (bank as u16 * 0x4000)) as usize;
                    bank_offset % self.prg_rom.len()
                } else {
                    let bank = self.prg_bank % prg_bank_count as u8;
                    let bank_offset = ((addr & 0x3FFF) + (bank as u16 * 0x4000)) as usize;
                    bank_offset % self.prg_rom.len()
                }
            },
            _ => unreachable!(),
        }
    }
    
    /// Get the address for CHR ROM/RAM access
    fn chr_addr(&self, addr: u16) -> usize {
        let chr_bank_count = self.chr.len() / self.chr_bank_size();
        
        match (self.control >> 4) & 0x01 {
            0 => {
                // 8KB mode
                let bank = (self.chr_bank_0 & 0x1E) % (chr_bank_count as u8 & 0xFE);
                let bank_offset = (addr + (bank as u16 * 0x2000)) as usize;
                bank_offset % self.chr.len()
            },
            1 => {
                // 4KB mode
                if addr < 0x1000 {
                    let bank = self.chr_bank_0 % chr_bank_count as u8;
                    let bank_offset = (addr + (bank as u16 * 0x1000)) as usize;
                    bank_offset % self.chr.len()
                } else {
                    let bank = self.chr_bank_1 % chr_bank_count as u8;
                    let bank_offset = ((addr & 0x0FFF) + (bank as u16 * 0x1000)) as usize;
                    bank_offset % self.chr.len()
                }
            },
            _ => unreachable!(),
        }
    }
    
    /// Update the mirroring mode based on the control register
    fn update_mirroring(&mut self) {
        self.mirroring = match self.control & 0x03 {
            0 => Mirroring::SingleScreenLower,
            1 => Mirroring::SingleScreenUpper,
            2 => Mirroring::Vertical,
            3 => Mirroring::Horizontal,
            _ => unreachable!(),
        };
    }
    
    /// Write to a mapper register
    fn write_register(&mut self, addr: u16, data: u8) {
        // Register area is selected by bits 13-14 of the address
        match (addr >> 13) & 0x03 {
            0 => {
                // Control register (0x8000-0x9FFF)
                self.control = data;
                self.update_mirroring();
            },
            1 => {
                // CHR bank 0 register (0xA000-0xBFFF)
                self.chr_bank_0 = data;
            },
            2 => {
                // CHR bank 1 register (0xC000-0xDFFF)
                self.chr_bank_1 = data;
            },
            3 => {
                // PRG bank register (0xE000-0xFFFF)
                self.prg_bank = data & 0x0F;
            },
            _ => unreachable!(),
        }
        
        debug!("MMC1 Register update: addr=${:04X}, data=${:02X}, control=${:02X}, chr0=${:02X}, chr1=${:02X}, prg=${:02X}",
              addr, data, self.control, self.chr_bank_0, self.chr_bank_1, self.prg_bank);
    }
}

impl Mapper for Mapper001 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                // PRG RAM
                let ram_addr = (addr & 0x1FFF) as usize;
                if ram_addr < self.prg_ram.len() {
                    self.prg_ram[ram_addr]
                } else {
                    0
                }
            },
            0x8000..=0xFFFF => {
                // PRG ROM
                let rom_addr = self.prg_addr(addr);
                self.prg_rom[rom_addr]
            },
            _ => 0,
        }
    }
    
    fn write_prg(&mut self, addr: u16, data: u8) {
        match addr {
            0x6000..=0x7FFF => {
                // PRG RAM
                let ram_addr = (addr & 0x1FFF) as usize;
                if ram_addr < self.prg_ram.len() {
                    self.prg_ram[ram_addr] = data;
                }
            },
            0x8000..=0xFFFF => {
                // Mapper registers
                // Reset on bit 7 set
                if (data & 0x80) != 0 {
                    self.shift_register = 0x10;
                    self.shift_count = 0;
                    self.control |= 0x0C;
                    return;
                }
                
                // Serial shift register
                self.shift_register >>= 1;
                self.shift_register |= (data & 0x01) << 4;
                self.shift_count += 1;
                
                // If 5 bits have been written, update the register
                if self.shift_count == 5 {
                    self.write_register(addr, self.shift_register);
                    self.shift_register = 0x10;
                    self.shift_count = 0;
                }
            },
            _ => {},
        }
    }
    
    fn read_chr(&self, addr: u16) -> u8 {
        let chr_addr = self.chr_addr(addr);
        self.chr[chr_addr]
    }
    
    fn write_chr(&mut self, addr: u16, data: u8) {
        if self.chr_is_ram {
            let chr_addr = self.chr_addr(addr);
            self.chr[chr_addr] = data;
        }
    }
    
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    
    fn irq_triggered(&self) -> bool {
        false // MMC1 doesn't generate IRQs
    }
    
    fn acknowledge_irq(&mut self) {
        // No IRQs in MMC1
    }
    
    fn notify_scanline(&mut self) {
        // No scanline counter in MMC1
    }
    
    fn reset(&mut self) {
        self.shift_register = 0x10;
        self.shift_count = 0;
        self.control = 0x0C;
        self.chr_bank_0 = 0;
        self.chr_bank_1 = 0;
        self.prg_bank = 0;
        self.update_mirroring();
    }
}

impl CartridgeTrait for Mapper001 {
    fn load_ram(&mut self, data: &[u8]) {
        if !data.is_empty() && data.len() <= self.prg_ram.len() {
            self.prg_ram[..data.len()].copy_from_slice(data);
        }
    }
}