//! Memory bus implementation for the NES
//!
//! The NES has a 16-bit address space (0x0000 - 0xFFFF) with various memory-mapped
//! components. This module implements the memory bus that connects all components
//! and handles the memory mapping.
//!
//! Memory Map:
//! - 0x0000 - 0x07FF: 2KB internal RAM
//! - 0x0800 - 0x1FFF: Mirrors of internal RAM
//! - 0x2000 - 0x2007: PPU registers
//! - 0x2008 - 0x3FFF: Mirrors of PPU registers
//! - 0x4000 - 0x4017: APU and I/O registers
//! - 0x4018 - 0x401F: APU and I/O functionality that is normally disabled
//! - 0x4020 - 0xFFFF: Cartridge space (PRG ROM, PRG RAM, and mapper registers)

use std::cell::RefCell;
use std::rc::Rc;
use log::trace;

use crate::cartridge::Cartridge;

/// Size of the internal RAM (2KB)
const RAM_SIZE: usize = 0x0800;

/// Represents the memory bus connecting all NES components
pub struct MemoryBus {
    /// Internal RAM (2KB)
    ram: [u8; RAM_SIZE],
    
    /// Cartridge connected to the system
    cartridge: Option<Rc<RefCell<Cartridge>>>,
    
    /// PPU registers (shared with PPU)
    pub ppu_registers: [u8; 8],
    
    /// APU and I/O registers
    pub apu_io_registers: [u8; 0x18],
    
    /// OAM DMA is in progress
    pub oam_dma_active: bool,
    
    /// Current OAM DMA address
    pub oam_dma_addr: u8,
    
    /// Address for OAM DMA transfer
    pub oam_dma_page: u8,
    
    /// NMI signal is pending
    nmi_pending: bool,
    
    /// IRQ signal is pending
    irq_pending: bool,
}

impl MemoryBus {
    /// Create a new memory bus
    pub fn new() -> Self {
        MemoryBus {
            ram: [0; RAM_SIZE],
            cartridge: None,
            ppu_registers: [0; 8],
            apu_io_registers: [0; 0x18],
            oam_dma_active: false,
            oam_dma_addr: 0,
            oam_dma_page: 0,
            nmi_pending: false,
            irq_pending: false,
        }
    }

    /// Reset the memory bus
    pub fn reset(&mut self) {
        self.ram = [0; RAM_SIZE];
        self.ppu_registers = [0; 8];
        self.apu_io_registers = [0; 0x18];
        self.oam_dma_active = false;
        self.oam_dma_addr = 0;
        self.oam_dma_page = 0;
        self.nmi_pending = false;
        self.irq_pending = false;
    }

    /// Insert a cartridge into the system
    pub fn insert_cartridge(&mut self, cartridge: Cartridge) {
        self.cartridge = Some(Rc::new(RefCell::new(cartridge)));
    }

    /// Remove the cartridge from the system
    pub fn remove_cartridge(&mut self) {
        self.cartridge = None;
    }

    /// Read a byte from memory at the specified address
    pub fn read(&mut self, addr: u16) -> u8 {
        match addr {
            // Internal RAM and mirrors
            0x0000..=0x1FFF => {
                let ram_addr = (addr & 0x07FF) as usize;
                self.ram[ram_addr]
            },
            
            // PPU registers and mirrors
            0x2000..=0x3FFF => {
                let reg_addr = ((addr - 0x2000) & 0x0007) as usize;
                self.read_ppu_register(reg_addr)
            },
            
            // APU and I/O registers
            0x4000..=0x4017 => {
                self.read_apu_io_register(addr)
            },
            
            // APU and I/O functionality (normally disabled)
            0x4018..=0x401F => {
                trace!("Read from disabled APU and I/O functionality: ${:04X}", addr);
                0
            },
            
            // Cartridge space
            0x4020..=0xFFFF => {
                if let Some(cart) = &self.cartridge {
                    cart.borrow_mut().read(addr)
                } else {
                    trace!("Read from cartridge space with no cartridge: ${:04X}", addr);
                    0
                }
            },
        }
    }

    /// Write a byte to memory at the specified address
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            // Internal RAM and mirrors
            0x0000..=0x1FFF => {
                let ram_addr = (addr & 0x07FF) as usize;
                self.ram[ram_addr] = value;
            },
            
            // PPU registers and mirrors
            0x2000..=0x3FFF => {
                let reg_addr = ((addr - 0x2000) & 0x0007) as usize;
                self.write_ppu_register(reg_addr, value);
            },
            
            // APU and I/O registers
            0x4000..=0x4017 => {
                self.write_apu_io_register(addr, value);
            },
            
            // APU and I/O functionality (normally disabled)
            0x4018..=0x401F => {
                trace!("Write to disabled APU and I/O functionality: ${:04X} = ${:02X}", addr, value);
            },
            
            // Cartridge space
            0x4020..=0xFFFF => {
                if let Some(cart) = &self.cartridge {
                    cart.borrow_mut().write(addr, value);
                } else {
                    trace!("Write to cartridge space with no cartridge: ${:04X} = ${:02X}", addr, value);
                }
            },
        }
    }

    /// Read from a PPU register
    fn read_ppu_register(&mut self, reg: usize) -> u8 {
        match reg {
            // PPUSTATUS ($2002)
            2 => {
                // Reading PPUSTATUS clears bit 7 (vblank) and resets the PPU address latch
                let value = self.ppu_registers[2];
                self.ppu_registers[2] &= 0x7F; // Clear VBlank flag
                // Reset PPU address latch handled in PPU module
                value
            },
            
            // OAMDATA ($2004)
            4 => {
                // Reading from OAMDATA during OAM DMA should return 0xFF
                if self.oam_dma_active {
                    0xFF
                } else {
                    self.ppu_registers[4]
                }
            },
            
            // PPUDATA ($2007)
            7 => {
                // Reading from PPUDATA should return from the PPU's internal buffer
                // And update the PPU address
                // This is handled in the PPU module
                self.ppu_registers[7]
            },
            
            // Other PPU registers
            _ => self.ppu_registers[reg],
        }
    }

    /// Write to a PPU register
    fn write_ppu_register(&mut self, reg: usize, value: u8) {
        match reg {
            // PPUCTRL ($2000)
            0 => {
                self.ppu_registers[0] = value;
                // If NMI enable bit is set and VBlank flag is set, trigger NMI
                if (value & 0x80) != 0 && (self.ppu_registers[2] & 0x80) != 0 {
                    self.nmi_pending = true;
                }
            },
            
            // PPUMASK ($2001)
            1 => {
                self.ppu_registers[1] = value;
            },
            
            // PPUSTATUS ($2002)
            2 => {
                // PPUSTATUS is read-only
                trace!("Attempted write to read-only PPUSTATUS: ${:02X}", value);
            },
            
            // OAMADDR ($2003)
            3 => {
                self.ppu_registers[3] = value;
            },
            
            // OAMDATA ($2004)
            4 => {
                self.ppu_registers[4] = value;
                // Update OAM handled in PPU module
            },
            
            // PPUSCROLL ($2005)
            5 => {
                self.ppu_registers[5] = value;
                // Scroll handling in PPU module
            },
            
            // PPUADDR ($2006)
            6 => {
                self.ppu_registers[6] = value;
                // PPU address handling in PPU module
            },
            
            // PPUDATA ($2007)
            7 => {
                self.ppu_registers[7] = value;
                // PPU data write handling in PPU module
            },
            
            _ => unreachable!("Invalid PPU register: {}", reg),
        }
    }

    /// Read from an APU or I/O register
    fn read_apu_io_register(&mut self, addr: u16) -> u8 {
        match addr {
            // Controller 1 ($4016)
            0x4016 => {
                // Controller 1 read handled in controller module
                self.apu_io_registers[0x16]
            },
            
            // Controller 2 ($4017)
            0x4017 => {
                // Controller 2 read handled in controller module
                self.apu_io_registers[0x17]
            },
            
            // Other APU and I/O registers
            _ => {
                let reg = (addr - 0x4000) as usize;
                self.apu_io_registers[reg]
            }
        }
    }

    /// Write to an APU or I/O register
    fn write_apu_io_register(&mut self, addr: u16, value: u8) {
        match addr {
            // OAM DMA ($4014)
            0x4014 => {
                self.oam_dma_page = value;
                self.oam_dma_active = true;
                self.oam_dma_addr = 0;
                // OAM DMA will be performed in the NES main loop
            },
            
            // Controller 1 ($4016)
            0x4016 => {
                // Controller strobe handling
                self.apu_io_registers[0x16] = value;
            },
            
            // Controller 2 and APU frame counter ($4017)
            0x4017 => {
                // APU frame counter and controller 2 handling
                self.apu_io_registers[0x17] = value;
            },
            
            // Other APU and I/O registers
            _ => {
                let reg = (addr - 0x4000) as usize;
                self.apu_io_registers[reg] = value;
            }
        }
    }

    /// Check if an NMI signal is pending and clear it
    pub fn peek_nmi(&self) -> bool {
        self.nmi_pending
    }

    /// Acknowledge and clear the NMI signal
    pub fn acknowledge_nmi(&mut self) {
        self.nmi_pending = false;
    }

    /// Check if an IRQ signal is pending
    pub fn peek_irq(&self) -> bool {
        self.irq_pending
    }

    /// Acknowledge and clear the IRQ signal
    pub fn acknowledge_irq(&mut self) {
        self.irq_pending = false;
    }

    /// Set the IRQ signal from the cartridge
    pub fn set_irq_from_cartridge(&mut self, value: bool) {
        self.irq_pending = value;
    }

    /// Perform OAM DMA transfer
    pub fn perform_oam_dma(&mut self) -> u32 {
        if !self.oam_dma_active {
            return 0;
        }
        
        // OAM DMA takes 513 or 514 CPU cycles (depending on whether it starts on an odd or even cycle)
        // We'll assume 514 for simplicity
        let dma_source_base = (self.oam_dma_page as u16) << 8;
        
        for i in 0..256 {
            let source_addr = dma_source_base + i;
            let value = self.read(source_addr);
            
            // Write to OAM through OAMDATA ($2004)
            self.write(0x2004, value);
        }
        
        self.oam_dma_active = false;
        
        // Return the number of cycles consumed
        514
    }
}