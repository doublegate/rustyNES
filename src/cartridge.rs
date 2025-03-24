//! NES cartridge implementation
//!
//! This module handles the NES cartridge format (iNES), including ROM/RAM banking
//! and mappers. The NES uses a cartridge system with separate PRG ROM (program code)
//! and CHR ROM/RAM (character/graphics data).

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use log::info;
use thiserror::Error;
use serde::{Serialize, Deserialize};

use crate::mappers::Mapper;
use crate::mappers::create_mapper;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Mapper implementation
    mapper: Rc<RefCell<Box<dyn Mapper>>>,
    
    /// Mirroring mode (from header, may be overridden by mapper)
    mirroring: Mirroring,
    
    /// Whether battery-backed RAM is present
    has_battery: bool,
    
    /// Whether NTSC or PAL is used
    is_pal: bool,
    
    /// PRG ROM size in bytes
    prg_rom_size: usize,
    
    /// CHR ROM size in bytes
    chr_rom_size: usize,
    
    /// PRG RAM size in bytes
    prg_ram_size: usize,
    
    /// Mapper number
    mapper_number: u8,
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
        let flags9 = data[9];
        
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
        
        // Check if this is an NES 2.0 format ROM
        let is_nes2 = (flags7 & 0x0C) == 0x08;
        
        // Check if PAL or NTSC
        let is_pal = (flags9 & 0x01) != 0;
        
        // Calculate PRG RAM size
        let prg_ram_size = if is_nes2 {
            // NES 2.0 format
            if data[10] & 0x0F == 0 {
                0
            } else {
                64 << ((data[10] & 0x0F) - 1)
            }
        } else {
            // iNES format
            if data[8] == 0 {
                8 * 1024 // Default to 8KB
            } else {
                data[8] as usize * 8 * 1024
            }
        };
        
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
        
        // For now, we only support mappers 0-4
        if mapper > 4 {
            return Err(ROMParseError::UnsupportedMapper(mapper));
        }
        
        // Load PRG ROM
        let prg_rom_start = INES_HEADER_SIZE + trainer_size;
        let prg_rom_end = prg_rom_start + prg_rom_size;
        let prg_rom = data[prg_rom_start..prg_rom_end].to_vec();
        
        // Load CHR ROM or create CHR RAM
        let chr_rom = if chr_rom_size == 0 {
            Vec::new() // CHR RAM will be created by the mapper
        } else {
            let chr_rom_start = prg_rom_end;
            let chr_rom_end = chr_rom_start + chr_rom_size;
            data[chr_rom_start..chr_rom_end].to_vec()
        };
        
        // Create PRG RAM
        let prg_ram = vec![0; prg_ram_size];
        
        // Determine CHR RAM size if CHR ROM is empty
        let chr_ram_size = if chr_rom_size == 0 {
            8 * 1024 // Default to 8KB
        } else {
            0
        };
        
        // Create mapper
        let mapper_impl = create_mapper(
            mapper,
            prg_rom,
            chr_rom,
            prg_ram,
            chr_ram_size,
            mirroring,
        );
        
        info!("Loaded cartridge - Mapper: {}, PRG ROM: {}KB, CHR {}: {}KB, Mirroring: {:?}, Battery: {}, TV System: {}",
             mapper, prg_rom_size / 1024,
             if chr_rom_size == 0 { "RAM" } else { "ROM" },
             if chr_rom_size == 0 { chr_ram_size } else { chr_rom_size } / 1024,
             mirroring, has_battery, if is_pal { "PAL" } else { "NTSC" });
        
        Ok(Cartridge {
            mapper: Rc::new(RefCell::new(mapper_impl)),
            mirroring,
            has_battery,
            is_pal,
            prg_rom_size,
            chr_rom_size,
            prg_ram_size,
            mapper_number: mapper,
        })
    }

    /// Read a byte from the cartridge (CPU space)
    pub fn read(&self, addr: u16) -> u8 {
        self.mapper.borrow().read_prg(addr)
    }

    /// Write a byte to the cartridge (CPU space)
    pub fn write(&self, addr: u16, value: u8) {
        self.mapper.borrow_mut().write_prg(addr, value);
    }

    /// Read a byte from the CHR ROM/RAM (PPU space)
    pub fn read_chr(&self, addr: u16) -> u8 {
        self.mapper.borrow().read_chr(addr)
    }

    /// Write a byte to the CHR ROM/RAM (PPU space)
    pub fn write_chr(&self, addr: u16, value: u8) {
        self.mapper.borrow_mut().write_chr(addr, value);
    }

    /// Get the current mirroring mode (may be overridden by mapper)
    pub fn get_mirroring(&self) -> Mirroring {
        self.mapper.borrow().mirroring()
    }

    /// Check if the mapper has triggered an IRQ
    pub fn irq_triggered(&self) -> bool {
        self.mapper.borrow().irq_triggered()
    }

    /// Acknowledge an IRQ
    pub fn acknowledge_irq(&self) {
        self.mapper.borrow_mut().acknowledge_irq();
    }

    /// Notify the mapper that a scanline has been completed
    pub fn notify_scanline(&self) {
        self.mapper.borrow_mut().notify_scanline();
    }

    /// Get the mapper number
    pub fn mapper_number(&self) -> u8 {
        self.mapper_number
    }

    /// Save the cartridge RAM to a byte vector (for battery-backed RAM)
    pub fn save_ram(&self) -> Vec<u8> {
        // This would be implemented to save the PRG RAM for battery-backed games
        Vec::new()
    }

    /// Load the cartridge RAM from a byte vector (for battery-backed RAM)
    pub fn load_ram(&self, data: &[u8]) {
        self.mapper.borrow_mut().load_ram(data);
    }
}

impl fmt::Debug for Cartridge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cartridge")
            .field("mapper", &self.mapper_number)
            .field("mirroring", &self.mirroring)
            .field("prg_rom_size", &self.prg_rom_size)
            .field("chr_rom_size", &self.chr_rom_size)
            .field("prg_ram_size", &self.prg_ram_size)
            .field("has_battery", &self.has_battery)
            .field("is_pal", &self.is_pal)
            .finish()
    }
}

pub trait CartridgeTrait {
    /// Load save RAM data
    fn load_ram(&mut self, _data: &[u8]) {
        // Default implementation does nothing
        // Override this in mappers that support save RAM
    }
}