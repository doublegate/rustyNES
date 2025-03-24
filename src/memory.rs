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

use crate::cartridge::{Cartridge, Mirroring};
use crate::ppu::PPU;

/// Size of the internal RAM (2KB)
const RAM_SIZE: usize = 0x0800;

/// Represents the memory bus connecting all NES components
pub struct MemoryBus {
    /// Internal RAM (2KB)
    ram: [u8; RAM_SIZE],
    
    /// Cartridge connected to the system
    cartridge: Option<Rc<RefCell<Cartridge>>>,
    
    /// PPU instance
    ppu: Rc<RefCell<PPU>>,
    
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
    pub fn new(ppu: Rc<RefCell<PPU>>) -> Self {
        MemoryBus {
            ram: [0; RAM_SIZE],
            cartridge: None,
            ppu,
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

    // Updated read method to use the mapper system
    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            // Internal RAM and mirrors
            0x0000..=0x1FFF => {
                let ram_addr = (addr & 0x07FF) as usize;
                self.ram[ram_addr]
            },
            
            // PPU registers and mirrors
            0x2000..=0x3FFF => {
                let reg = ((addr - 0x2000) & 0x0007) as usize;
                self.ppu_registers[reg]
            },
            
            // APU and I/O registers
            0x4000..=0x4017 => {
                let reg = (addr & 0x1F) as usize;
                match addr {
                    0x4016 => {
                        // Controller 1 read
                        self.apu_io_registers[22] & 0xE0
                    },
                    0x4017 => {
                        // Controller 2 read
                        self.apu_io_registers[23] & 0xE0
                    },
                    _ => self.apu_io_registers[reg],
                }
            },
            
            // APU and I/O functionality (normally disabled)
            0x4018..=0x401F => {
                trace!("Read from disabled APU and I/O functionality: ${:04X}", addr);
                0
            },
            
            // Cartridge space
            0x4020..=0xFFFF => {
                if let Some(cart) = &self.cartridge {
                    cart.borrow().read(addr)
                } else {
                    trace!("Read from cartridge space with no cartridge: ${:04X}", addr);
                    0
                }
            },
        }
    }

    // Updated write method to use the mapper system
    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            // Internal RAM and mirrors
            0x0000..=0x1FFF => {
                let ram_addr = (addr & 0x07FF) as usize;
                self.ram[ram_addr] = value;
            },
            
            // PPU registers and mirrors
            0x2000..=0x3FFF => {
                let reg = ((addr - 0x2000) & 0x0007) as usize;
                self.ppu_registers[reg] = value;
                
                // Handle special PPU register writes
                match reg {
                    // PPUCTRL ($2000)
                    0 => {
                        let mut ppu = self.ppu.borrow_mut();
                        ppu.nmi_output = (value & 0x80) != 0;
                        if ppu.nmi_output && ppu.nmi_occurred {
                            self.nmi_pending = true;
                        }
                    },
                    // Other registers...
                    _ => {}
                }
            },
            
            // APU and I/O registers
            0x4000..=0x4017 => {
                let reg = (addr & 0x1F) as usize;
                match addr {
                    0x4016 => {
                        // Controller 1 write
                        self.apu_io_registers[22] = value;
                    },
                    0x4017 => {
                        // Controller 2 write
                        self.apu_io_registers[23] = value;
                    },
                    _ => self.apu_io_registers[reg] = value,
                }
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
                self.ppu.borrow_mut().w = false; // Reset write toggle
                value
            },
            
            // OAMDATA ($2004)
            4 => {
                // Reading from OAMDATA during OAM DMA should return 0xFF
                if self.oam_dma_active {
                    0xFF
                } else {
                    self.ppu.borrow_mut().oam[self.ppu_registers[3] as usize]
                }
            },
            
            // PPUDATA ($2007)
            7 => {
                // Get current address and buffer value
                let addr;
                let result;
                {
                    let ppu = self.ppu.borrow();
                    addr = ppu.v;
                    result = ppu.data_buffer;
                }
                
                // Update VRAM address
                {
                    let mut ppu = self.ppu.borrow_mut();
                    ppu.v = ppu.v.wrapping_add(if (self.ppu_registers[0] & 0x04) != 0 { 32 } else { 1 });
                }
                
                // Handle the read based on address
                let value = if addr >= 0x3F00 {
                    // Palette reads are immediate
                    let palette_addr = (addr & 0x1F) as usize;
                    self.ppu.borrow().palette_ram[palette_addr]
                } else if addr < 0x2000 {
                    // Pattern tables (CHR ROM/RAM)
                    if let Some(cart) = &self.cartridge {
                        cart.borrow().read_chr(addr)
                    } else {
                        0
                    }
                } else if addr < 0x3000 {
                    // Nametables
                    let mirrored_addr = self.mirror_vram_addr(addr);
                    self.ppu.borrow().vram[mirrored_addr as usize]
                } else {
                    // Mirrors of nametables
                    let mirrored_addr = self.mirror_vram_addr(addr - 0x1000);
                    self.ppu.borrow().vram[mirrored_addr as usize]
                };
                
                // Update data buffer
                if addr < 0x3F00 {
                    // For non-palette reads, return the buffer and update it
                    {
                        let mut ppu = self.ppu.borrow_mut();
                        ppu.data_buffer = value;
                    }
                    result
                } else {
                    // For palette reads, return the value immediately
                    {
                        let mut ppu = self.ppu.borrow_mut();
                        ppu.data_buffer = value;
                    }
                    value
                }
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
                let nmi_change = (self.ppu_registers[0] ^ value) & 0x80;
                self.ppu_registers[0] = value;
                
                {
                    let mut ppu = self.ppu.borrow_mut();
                    ppu.t = (ppu.t & 0xF3FF) | ((value as u16 & 0x03) << 10);
                    
                    // Update NMI output flag based on bit 7
                    ppu.nmi_output = (value & 0x80) != 0;
                }
                
                // If NMI enable changes from 0 to 1 during VBlank, trigger NMI
                if nmi_change != 0 && (value & 0x80) != 0 && (self.ppu_registers[2] & 0x80) != 0 {
                    self.ppu.borrow_mut().nmi_occurred = true;
                    self.set_nmi_pending(true);
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
                let oam_addr = self.ppu_registers[3] as usize;
                {
                    let mut ppu = self.ppu.borrow_mut();
                    ppu.oam[oam_addr] = value;
                }
                self.ppu_registers[3] = self.ppu_registers[3].wrapping_add(1);
            },
            
            // PPUSCROLL ($2005)
            5 => {
                let ppu_w = self.ppu.borrow().w;
                if !ppu_w {
                    // First write (X scroll)
                    {
                        let mut ppu = self.ppu.borrow_mut();
                        ppu.t = (ppu.t & 0xFFE0) | ((value as u16) >> 3);
                        ppu.x = value & 0x07;
                        ppu.w = true;
                    }
                } else {
                    // Second write (Y scroll)
                    {
                        let mut ppu = self.ppu.borrow_mut();
                        ppu.t = (ppu.t & 0x8FFF) | ((value as u16 & 0x07) << 12);
                        ppu.t = (ppu.t & 0xFC1F) | ((value as u16 & 0xF8) << 2);
                        ppu.w = false;
                    }
                }
            },
            
            // PPUADDR ($2006)
            6 => {
                let ppu_w = self.ppu.borrow().w;
                if !ppu_w {
                    // First write (high byte)
                    {
                        let mut ppu = self.ppu.borrow_mut();
                        ppu.t = (ppu.t & 0x00FF) | ((value as u16 & 0x3F) << 8);
                        ppu.w = true;
                    }
                } else {
                    // Second write (low byte)
                    {
                        let mut ppu = self.ppu.borrow_mut();
                        ppu.t = (ppu.t & 0xFF00) | value as u16;
                        ppu.v = ppu.t;
                        ppu.w = false;
                    }
                }
            },
            
            // PPUDATA ($2007)
            7 => {
                let addr;
                {
                    let ppu = self.ppu.borrow();
                    addr = ppu.v;
                }
                
                let increment = if (self.ppu_registers[0] & 0x04) != 0 { 32 } else { 1 };
                
                // Write to appropriate memory
                if addr >= 0x3F00 {
                    // Palette RAM
                    let palette_addr = (addr & 0x1F) as usize;
                    self.ppu.borrow_mut().palette_ram[palette_addr] = value;
                } else if addr < 0x2000 {
                    // Pattern tables (CHR ROM/RAM)
                    if let Some(cart) = &self.cartridge {
                        cart.borrow_mut().write_chr(addr, value);
                    }
                } else if addr < 0x3000 {
                    // Nametables
                    let mirrored_addr = self.mirror_vram_addr(addr);
                    self.ppu.borrow_mut().vram[mirrored_addr as usize] = value;
                } else {
                    // Mirrors of nametables
                    let mirrored_addr = self.mirror_vram_addr(addr - 0x1000);
                    self.ppu.borrow_mut().vram[mirrored_addr as usize] = value;
                }
                
                // Update VRAM address after write
                {
                    let mut ppu = self.ppu.borrow_mut();
                    ppu.v = ppu.v.wrapping_add(increment);
                }
            },
            
            _ => {}
        }
    }

    /// Mirror VRAM address based on current mirroring mode
    fn mirror_vram_addr(&self, addr: u16) -> u16 {
        let addr = addr & 0x2FFF;
        let mirroring = if let Some(cart) = &self.cartridge {
            cart.borrow().get_mirroring()
        } else {
            Mirroring::Horizontal
        };
        
        match mirroring {
            Mirroring::Horizontal => {
                // Nametable layout:
                // A A
                // B B
                let table = (addr >> 11) & 0x01;
                (table << 10) | (addr & 0x03FF)
            },
            Mirroring::Vertical => {
                // Nametable layout:
                // A B
                // A B
                let table = (addr >> 10) & 0x01;
                (table << 10) | (addr & 0x03FF)
            },
            Mirroring::SingleScreenLower => {
                // Single screen, lower bank
                addr & 0x03FF
            },
            Mirroring::SingleScreenUpper => {
                // Single screen, upper bank
                0x0400 | (addr & 0x03FF)
            },
            Mirroring::FourScreen => {
                // Four screen - no mirroring
                addr & 0x0FFF
            },
        }
    }

    /// Read from an APU or I/O register
    fn read_apu_io_register(&mut self, addr: u16) -> u8 {
        match addr {
            // Controller 1 ($4016)
            0x4016 => {
                // Controller 1 read is handled externally by providing the value
                // This will be connected properly in the NES run_frame method
                self.apu_io_registers[22] & 0xE0
            },
            
            // Controller 2 ($4017)
            0x4017 => {
                // Controller 2 read is handled externally
                self.apu_io_registers[23] & 0xE0
            },
            
            // Other APU and I/O registers
            _ => {
                let reg = (addr & 0x1F) as usize;
                if reg < self.apu_io_registers.len() {
                    self.apu_io_registers[reg]
                } else {
                    0 // Default value for out-of-range registers
                }
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
            
            // Controller 1 and 2 ($4016)
            0x4016 => {
                // Store strobe value, actual controller update happens in NES
                self.apu_io_registers[22] = value;
            },
            
            // APU frame counter ($4017)
            0x4017 => {
                // APU frame counter handling
                self.apu_io_registers[23] = value;
            },
            
            // Other APU and I/O registers
            _ => {
                let reg = (addr & 0x1F) as usize;
                if reg < self.apu_io_registers.len() {
                    self.apu_io_registers[reg] = value;
                }
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
        
        let dma_source_base = u16::from(self.oam_dma_page) << 8;
        
        for i in 0..256 {
            let source_addr = dma_source_base.wrapping_add(i);
            let value = self.read(source_addr);
            
            // Write directly to OAM (avoid going through OAMDATA/$2004)
            let oam_addr = self.oam_dma_addr.wrapping_add(i as u8) as usize;
            self.ppu.borrow_mut().oam[oam_addr % 256] = value;
        }
        
        self.oam_dma_active = false;
        
        // OAM DMA takes 513 or 514 CPU cycles (depending on whether it starts on an odd or even cycle)
        // We'll use 514 for simplicity
        514
    }

    /// Get a reference to the RAM
    pub fn get_ram(&self) -> &[u8] {
        &self.ram
    }

    /// Get a mutable reference to the RAM
    pub fn get_ram_mut(&mut self) -> &mut [u8] {
        &mut self.ram
    }

    /// Copy data into RAM
    pub fn copy_ram(&mut self, data: &[u8]) {
        if data.len() == self.ram.len() {
            self.ram.copy_from_slice(data);
        }
    }

    /// Get the current cartridge
    pub fn get_cartridge(&self) -> Option<Rc<RefCell<Cartridge>>> {
        self.cartridge.clone()
    }

    pub fn get_nmi_pending(&self) -> bool {
        self.nmi_pending
    }

    pub fn get_irq_pending(&self) -> bool {
        self.irq_pending
    }

    pub fn set_nmi_pending(&mut self, value: bool) {
        self.nmi_pending = value;
    }

    pub fn set_irq_pending(&mut self, value: bool) {
        self.irq_pending = value;
    }
}