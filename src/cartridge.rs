//! NES cartridge implementation
//!
//! This module handles the NES cartridge format (iNES), including ROM/RAM banking
//! and mappers. The NES uses a cartridge system with separate PRG ROM (program code)
//! and CHR ROM/RAM (character/graphics data).

use std::fmt;
use log::{debug, info, warn};
use thiserror::Error;

/// Size of the iNES header
const INES_HEADER_SIZE: usize = 16;

/// Size of a PRG ROM bank (16KB)
const PRG_ROM_BANK_SIZE: usize = 16 * 1024;

/// Size of a CHR ROM/RAM bank (8KB)
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Errors that can occur when parsing ROM files
#[derive(Error, Debug)]
pub enum ROMParseError {
    #[error("Invalid iNES header")]
    InvalidHeader,
    
    #[error("Unsupported mapper: {0}")]
    UnsupportedMapper(u8),
    
    #[error("Invalid ROM size")]
    InvalidRomSize,
    
    #[error("Trainer present but not supported")]
    TrainerNotSupported,
}

/// Mirroring modes for the NES
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Mirroring {
    /// Horizontal mirroring (vertical arrangement of nametables)
    Horizontal,
    
    /// Vertical mirroring (horizontal arrangement of nametables)
    Vertical,
    
    /// Four-screen mirroring (no mirroring)
    FourScreen,
    
    /// Single-screen mirroring, lower bank
    SingleScreenLower,
    
    /// Single-screen mirroring, upper bank
    SingleScreenUpper,
}

/// Represents an NES cartridge
pub struct Cartridge {
    /// PRG ROM data
    prg_rom: Vec<u8>,
    
    /// PRG RAM data
    prg_ram: Vec<u8>,
    
    /// CHR ROM/RAM data
    chr: Vec<u8>,
    
    /// Whether CHR is RAM (writable) or ROM (read-only)
    chr_is_ram: bool,
    
    /// Mapper number
    mapper: u8,
    
    /// Mirroring mode
    mirroring: Mirroring,
    
    /// Whether battery-backed RAM is present
    has_battery: bool,
    
    /// Current PRG ROM bank for bankable region
    prg_bank: usize,
    
    /// Current CHR ROM/RAM bank
    chr_bank: usize,
}

impl Cartridge {
    /// Create a cartridge from ROM data in iNES format
    pub fn from_bytes(data: &[u8]) -> Result<Self, ROMParseError> {
        // Check for valid iNES header
        if data.len() < INES_HEADER_SIZE || data[0..4] != [0x4E, 0x45, 0x53, 0x1A] {
            return Err(ROMParseError::InvalidHeader);
        }
        
        // Parse header
        let prg_rom_size = data[4] as usize * PRG_ROM_BANK_SIZE;
        let chr_rom_size = data[5] as usize * CHR_BANK_SIZE;
        
        let flags6 = data[6];
        let flags7 = data[7];
        
        let mirroring = if (flags6 & 0x08) != 0 {
            Mirroring::FourScreen
        } else if (flags6 & 0x01) != 0 {
            Mirroring::Vertical
        } else {
            Mirroring::Horizontal
        };
        
        let has_battery = (flags6 & 0x02) != 0;
        let has_trainer = (flags6 & 0x04) != 0;
        
        // Extract mapper number
        let mapper_low = (flags6 >> 4) & 0x0F;
        let mapper_high = flags7 & 0xF0;
        let mapper = mapper_high | mapper_low;
        
        // Check if trainer is present (512 bytes before PRG ROM)
        let trainer_size = if has_trainer { 512 } else { 0 };
        
        // Check total file size
        let expected_size = INES_HEADER_SIZE + trainer_size + prg_rom_size + chr_rom_size;
        if data.len() < expected_size {
            return Err(ROMParseError::InvalidRomSize);
        }
        
        // For now, we don't support trainers
        if has_trainer {
            return Err(ROMParseError::TrainerNotSupported);
        }
        
        // For now, we only support mappers 0 and 1 (NROM and MMC1)
        if mapper != 0 && mapper != 1 {
            return Err(ROMParseError::UnsupportedMapper(mapper));
        }
        
        // Load PRG ROM
        let prg_rom_start = INES_HEADER_SIZE + trainer_size;
        let prg_rom_end = prg_rom_start + prg_rom_size;
        let prg_rom = data[prg_rom_start..prg_rom_end].to_vec();
        
        // Load CHR ROM or create CHR RAM
        let chr_is_ram = chr_rom_size == 0;
        let chr = if chr_is_ram {
            // Create 8KB of CHR RAM
            vec![0; CHR_BANK_SIZE]
        } else {
            let chr_rom_start = prg_rom_end;
            let chr_rom_end = chr_rom_start + chr_rom_size;
            data[chr_rom_start..chr_rom_end].to_vec()
        };
        
        // Create PRG RAM (8KB)
        let prg_ram = vec![0; 8 * 1024];
        
        info!("Loaded cartridge - Mapper: {}, PRG ROM: {}KB, CHR {}: {}KB, Mirroring: {:?}, Battery: {}",
             mapper, prg_rom_size / 1024, if chr_is_ram { "RAM" } else { "ROM" }, 
             chr.len() / 1024, mirroring, has_battery);
        
        Ok(Cartridge {
            prg_rom,
            prg_ram,
            chr,
            chr_is_ram,
            mapper,
            mirroring,
            has_battery,
            prg_bank: 0,
            chr_bank: 0,
        })
    }

    /// Read a byte from the cartridge
    pub fn read(&mut self, addr: u16) -> u8 {
        match addr {
            // PRG ROM - 16KB (single bank) or 32KB (fixed)
            0x8000..=0xFFFF => {
                match self.mapper {
                    // Mapper 0 (NROM)
                    0 => {
                        // For 16KB PRG ROM, mirror 0x8000-0xBFFF to 0xC000-0xFFFF
                        let effective_addr = if self.prg_rom.len() == PRG_ROM_BANK_SIZE {
                            (addr & 0x3FFF) as usize
                        } else {
                            (addr & 0x7FFF) as usize
                        };
                        
                        if effective_addr < self.prg_rom.len() {
                            self.prg_rom[effective_addr]
                        } else {
                            warn!("Read from invalid PRG ROM address: ${:04X}", addr);
                            0
                        }
                    },
                    
                    // Mapper 1 (MMC1)
                    1 => {
                        // Simplified MMC1 implementation
                        match addr {
                            // First 16KB bank (switchable or fixed)
                            0x8000..=0xBFFF => {
                                let bank_addr = (self.prg_bank * PRG_ROM_BANK_SIZE) + ((addr - 0x8000) as usize);
                                if bank_addr < self.prg_rom.len() {
                                    self.prg_rom[bank_addr]
                                } else {
                                    warn!("Read from invalid PRG ROM bank address: ${:04X}", addr);
                                    0
                                }
                            },
                            
                            // Last 16KB bank (fixed to last bank or switchable)
                            0xC000..=0xFFFF => {
                                let last_bank = (self.prg_rom.len() / PRG_ROM_BANK_SIZE) - 1;
                                let bank_addr = (last_bank * PRG_ROM_BANK_SIZE) + ((addr - 0xC000) as usize);
                                if bank_addr < self.prg_rom.len() {
                                    self.prg_rom[bank_addr]
                                } else {
                                    warn!("Read from invalid PRG ROM last bank address: ${:04X}", addr);
                                    0
                                }
                            },
                            
                            _ => unreachable!(),
                        }
                    },
                    
                    _ => {
                        warn!("Read from unsupported mapper {} at address ${:04X}", self.mapper, addr);
                        0
                    }
                }
            },
            
            // PRG RAM - 8KB
            0x6000..=0x7FFF => {
                let ram_addr = (addr - 0x6000) as usize;
                if ram_addr < self.prg_ram.len() {
                    self.prg_ram[ram_addr]
                } else {
                    warn!("Read from invalid PRG RAM address: ${:04X}", addr);
                    0
                }
            },
            
            _ => {
                warn!("Read from invalid cartridge address: ${:04X}", addr);
                0
            }
        }
    }

    /// Write a byte to the cartridge
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            // PRG ROM / Mapper registers
            0x8000..=0xFFFF => {
                match self.mapper {
                    // Mapper 0 (NROM)
                    0 => {
                        // PRG ROM is read-only
                        warn!("Attempted write to read-only PRG ROM: ${:04X} = ${:02X}", addr, value);
                    },
                    
                    // Mapper 1 (MMC1)
                    1 => {
                        // Writing to any address in 0x8000-0xFFFF updates mapper registers
                        self.update_mmc1_registers(addr, value);
                    },
                    
                    _ => {
                        warn!("Write to unsupported mapper {} at address ${:04X} = ${:02X}", 
                             self.mapper, addr, value);
                    }
                }
            },
            
            // PRG RAM - 8KB
            0x6000..=0x7FFF => {
                let ram_addr = (addr - 0x6000) as usize;
                if ram_addr < self.prg_ram.len() {
                    self.prg_ram[ram_addr] = value;
                } else {
                    warn!("Write to invalid PRG RAM address: ${:04X} = ${:02X}", addr, value);
                }
            },
            
            _ => {
                warn!("Write to invalid cartridge address: ${:04X} = ${:02X}", addr, value);
            }
        }
    }

    /// Update MMC1 registers through serial writes
    fn update_mmc1_registers(&mut self, addr: u16, value: u8) {
        // MMC1 register updates are not implemented in this simplified version
        // In a complete implementation, this would handle the MMC1 shift register
        // and update PRG/CHR banking and mirroring accordingly
        
        debug!("MMC1 register write: ${:04X} = ${:02X}", addr, value);
        
        // Reset signal if bit 7 is set
        if (value & 0x80) != 0 {
            // Reset MMC1 registers
            self.prg_bank = 0;
            return;
        }
        
        // Change PRG bank for demonstration purposes
        // This is not how MMC1 actually works, but it's a simplification
        if addr >= 0xA000 && addr <= 0xBFFF {
            self.prg_bank = (value as usize) % (self.prg_rom.len() / PRG_ROM_BANK_SIZE);
            debug!("Changed PRG bank to {}", self.prg_bank);
        }
    }

    /// Get the current mirroring mode
    pub fn get_mirroring(&self) -> Mirroring {
        self.mirroring
    }

    /// Read a byte from the CHR ROM/RAM
    pub fn read_chr(&self, addr: u16) -> u8 {
        if addr < 0x2000 {
            let chr_addr = addr as usize;
            if chr_addr < self.chr.len() {
                self.chr[chr_addr]
            } else {
                warn!("Read from invalid CHR address: ${:04X}", addr);
                0
            }
        } else {
            warn!("Read from invalid CHR address: ${:04X}", addr);
            0
        }
    }

    /// Write a byte to the CHR ROM/RAM
    pub fn write_chr(&mut self, addr: u16, value: u8) {
        if addr < 0x2000 {
            let chr_addr = addr as usize;
            if chr_addr < self.chr.len() {
                if self.chr_is_ram {
                    self.chr[chr_addr] = value;
                } else {
                    warn!("Attempted write to read-only CHR ROM: ${:04X} = ${:02X}", addr, value);
                }
            } else {
                warn!("Write to invalid CHR address: ${:04X} = ${:02X}", addr, value);
            }
        } else {
            warn!("Write to invalid CHR address: ${:04X} = ${:02X}", addr, value);
        }
    }
}

impl fmt::Debug for Cartridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cartridge")
            .field("mapper", &self.mapper)
            .field("mirroring", &self.mirroring)
            .field("prg_rom_size", &self.prg_rom.len())
            .field("chr_size", &self.chr.len())
            .field("chr_is_ram", &self.chr_is_ram)
            .field("has_battery", &self.has_battery)
            .finish()
    }
}