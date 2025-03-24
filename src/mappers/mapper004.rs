//! Mapper 004 (MMC3) implementation
//!
//! This mapper features PRG ROM banking, CHR ROM banking, and configurable mirroring.
//! It also has an IRQ counter that can trigger on scanlines.
//!
//! Used by games like Super Mario Bros. 2/3, Mega Man 3-6, Kirby's Adventure, etc.
//!
//! Memory map:
//! - PRG ROM: Two switchable 8KB banks + one fixed 8KB bank + one switchable 8KB bank
//! - PRG RAM: 8KB (0x6000-0x7FFF)
//! - CHR ROM/RAM: Six switchable 1KB banks + two switchable 1KB banks

use crate::cartridge::{Mirroring, CartridgeTrait};
use super::Mapper;

pub struct Mapper004 {
    /// PRG ROM data
    prg_rom: Vec<u8>,
    
    /// CHR ROM/RAM data
    chr: Vec<u8>,
    
    /// PRG RAM data
    prg_ram: Vec<u8>,
    
    /// Whether CHR is RAM (writable) or ROM (read-only)
    chr_is_ram: bool,
    
    /// Current bank select (0-7)
    bank_select: u8,
    
    /// Current PRG ROM bank mode (0-1)
    prg_mode: u8,
    
    /// Current CHR ROM bank mode (0-1)
    chr_mode: u8,
    
    /// Bank registers (R0-R7)
    bank_registers: [u8; 8],
    
    /// Mirroring mode
    mirroring: Mirroring,
    
    /// IRQ counter
    irq_counter: u8,
    
    /// IRQ counter reload value
    irq_latch: u8,
    
    /// IRQ enabled flag
    irq_enabled: bool,
    
    /// IRQ pending flag
    irq_pending: bool,
    
    /// Reload flag (true = reload on next clock)
    irq_reload: bool,
    
    /// PRG RAM enable/protect
    prg_ram_protect: [bool; 2],
}

impl Mapper004 {
    /// Create a new Mapper004 instance
    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, prg_ram: Vec<u8>, mirroring: Mirroring) -> Self {
        let chr_is_ram = chr_rom.is_empty();
        let chr = if chr_is_ram {
            vec![0; 8 * 1024] // 8KB CHR RAM
        } else {
            chr_rom
        };
        
        Mapper004 {
            prg_rom,
            chr,
            prg_ram,
            chr_is_ram,
            bank_select: 0,
            prg_mode: 0,
            chr_mode: 0,
            bank_registers: [0; 8],
            mirroring,
            irq_counter: 0,
            irq_latch: 0,
            irq_enabled: false,
            irq_pending: false,
            irq_reload: false,
            prg_ram_protect: [false, false],
        }
    }
    
    /// Get the PRG ROM bank address for the specified bank number
    fn get_prg_bank_addr(&self, bank: usize) -> usize {
        let prg_bank_size = 8 * 1024;
        let prg_bank_count = self.prg_rom.len() / prg_bank_size;
        (bank % prg_bank_count) * prg_bank_size
    }
    
    /// Get the CHR ROM/RAM bank address for the specified bank number
    fn get_chr_bank_addr(&self, bank: usize) -> usize {
        let chr_bank_size = 1 * 1024;
        let chr_bank_count = self.chr.len() / chr_bank_size;
        (bank % chr_bank_count) * chr_bank_size
    }
    
    /// Map a CPU address to a PRG ROM address
    fn map_prg_addr(&self, addr: u16) -> usize {
        let bank_size = 8 * 1024;
        let prg_bank_count = self.prg_rom.len() / bank_size;
        let last_bank = prg_bank_count - 1;
        
        match addr {
            0x8000..=0x9FFF => {
                // Bank 0 (switchable or fixed to second-last bank)
                if self.prg_mode == 0 {
                    // R6 selects bank
                    let bank = self.bank_registers[6] as usize;
                    self.get_prg_bank_addr(bank) + (addr & 0x1FFF) as usize
                } else {
                    // Fixed to second-last bank
                    self.get_prg_bank_addr(last_bank - 1) + (addr & 0x1FFF) as usize
                }
            },
            0xA000..=0xBFFF => {
                // Bank 1 (always R7)
                let bank = self.bank_registers[7] as usize;
                self.get_prg_bank_addr(bank) + (addr & 0x1FFF) as usize
            },
            0xC000..=0xDFFF => {
                // Bank 2 (fixed to second-last bank or switchable)
                if self.prg_mode == 0 {
                    // Fixed to second-last bank
                    self.get_prg_bank_addr(last_bank - 1) + (addr & 0x1FFF) as usize
                } else {
                    // R6 selects bank
                    let bank = self.bank_registers[6] as usize;
                    self.get_prg_bank_addr(bank) + (addr & 0x1FFF) as usize
                }
            },
            0xE000..=0xFFFF => {
                // Bank 3 (fixed to last bank)
                self.get_prg_bank_addr(last_bank) + (addr & 0x1FFF) as usize
            },
            _ => 0,
        }
    }
    
    /// Map a PPU address to a CHR ROM/RAM address
    fn map_chr_addr(&self, addr: u16) -> usize {
        match addr {
            0x0000..=0x07FF => {
                // Bank 0 (R0 or R2)
                if self.chr_mode == 0 {
                    // R0 selects bank
                    let bank = self.bank_registers[0] as usize & 0xFE;
                    self.get_chr_bank_addr(bank) + (addr & 0x07FF) as usize
                } else {
                    // R2 selects bank
                    let bank = self.bank_registers[2] as usize;
                    self.get_chr_bank_addr(bank) + (addr & 0x03FF) as usize
                }
            },
            0x0800..=0x0FFF => {
                // Bank 1 (R0+1 or R3)
                if self.chr_mode == 0 {
                    // R0+1 selects bank
                    let bank = (self.bank_registers[0] as usize & 0xFE) + 1;
                    self.get_chr_bank_addr(bank) + (addr & 0x07FF) as usize
                } else {
                    // R3 selects bank
                    let bank = self.bank_registers[3] as usize;
                    self.get_chr_bank_addr(bank) + (addr & 0x03FF) as usize
                }
            },
            0x1000..=0x13FF => {
                // Bank 2 (R1 or R4)
                if self.chr_mode == 0 {
                    // R1 selects bank
                    let bank = self.bank_registers[1] as usize & 0xFE;
                    self.get_chr_bank_addr(bank) + (addr & 0x07FF) as usize
                } else {
                    // R4 selects bank
                    let bank = self.bank_registers[4] as usize;
                    self.get_chr_bank_addr(bank) + (addr & 0x03FF) as usize
                }
            },
            0x1400..=0x17FF => {
                // Bank 3 (R1+1 or R5)
                if self.chr_mode == 0 {
                    // R1+1 selects bank
                    let bank = (self.bank_registers[1] as usize & 0xFE) + 1;
                    self.get_chr_bank_addr(bank) + (addr & 0x07FF) as usize
                } else {
                    // R5 selects bank
                    let bank = self.bank_registers[5] as usize;
                    self.get_chr_bank_addr(bank) + (addr & 0x03FF) as usize
                }
            },
            0x1800..=0x1BFF => {
                // Bank 4 (R2 or R0)
                if self.chr_mode == 0 {
                    // R2 selects bank
                    let bank = self.bank_registers[2] as usize;
                    self.get_chr_bank_addr(bank) + (addr & 0x03FF) as usize
                } else {
                    // R0 selects bank
                    let bank = self.bank_registers[0] as usize & 0xFE;
                    self.get_chr_bank_addr(bank) + (addr & 0x07FF) as usize
                }
            },
            0x1C00..=0x1FFF => {
                // Bank 5 (R3 or R0+1)
                if self.chr_mode == 0 {
                    // R3 selects bank
                    let bank = self.bank_registers[3] as usize;
                    self.get_chr_bank_addr(bank) + (addr & 0x03FF) as usize
                } else {
                    // R0+1 selects bank
                    let bank = (self.bank_registers[0] as usize & 0xFE) + 1;
                    self.get_chr_bank_addr(bank) + (addr & 0x07FF) as usize
                }
            },
            _ => 0,
        }
    }
}

impl Mapper for Mapper004 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                // PRG RAM
                if self.prg_ram_protect[0] {
                    let ram_addr = (addr & 0x1FFF) as usize;
                    if ram_addr < self.prg_ram.len() {
                        self.prg_ram[ram_addr]
                    } else {
                        0
                    }
                } else {
                    0
                }
            },
            0x8000..=0xFFFF => {
                // PRG ROM
                let rom_addr = self.map_prg_addr(addr);
                if rom_addr < self.prg_rom.len() {
                    self.prg_rom[rom_addr]
                } else {
                    0
                }
            },
            _ => 0,
        }
    }
    
    fn write_prg(&mut self, addr: u16, data: u8) {
        match addr {
            0x6000..=0x7FFF => {
                // PRG RAM
                if self.prg_ram_protect[0] && !self.prg_ram_protect[1] {
                    let ram_addr = (addr & 0x1FFF) as usize;
                    if ram_addr < self.prg_ram.len() {
                        self.prg_ram[ram_addr] = data;
                    }
                }
            },
            0x8000..=0x9FFF => {
                if addr & 0x01 == 0 {
                    // Bank select (even address)
                    self.bank_select = data & 0x07;
                    self.prg_mode = (data >> 6) & 0x01;
                    self.chr_mode = (data >> 7) & 0x01;
                } else {
                    // Bank data (odd address)
                    self.bank_registers[self.bank_select as usize] = data;
                }
            },
            0xA000..=0xBFFF => {
                if addr & 0x01 == 0 {
                    // Mirroring (even address)
                    self.mirroring = if (data & 0x01) == 0 {
                        Mirroring::Vertical
                    } else {
                        Mirroring::Horizontal
                    };
                } else {
                    // PRG RAM protect (odd address)
                    self.prg_ram_protect[0] = (data & 0x80) != 0;
                    self.prg_ram_protect[1] = (data & 0x40) != 0;
                }
            },
            0xC000..=0xDFFF => {
                if addr & 0x01 == 0 {
                    // IRQ latch (even address)
                    self.irq_latch = data;
                } else {
                    // IRQ reload (odd address)
                    self.irq_reload = true;
                }
            },
            0xE000..=0xFFFF => {
                if addr & 0x01 == 0 {
                    // IRQ disable (even address)
                    self.irq_enabled = false;
                    self.irq_pending = false;
                } else {
                    // IRQ enable (odd address)
                    self.irq_enabled = true;
                }
            },
            _ => {},
        }
    }
    
    fn read_chr(&self, addr: u16) -> u8 {
        let chr_addr = self.map_chr_addr(addr);
        if chr_addr < self.chr.len() {
            self.chr[chr_addr]
        } else {
            0
        }
    }
    
    fn write_chr(&mut self, addr: u16, data: u8) {
        if self.chr_is_ram {
            let chr_addr = self.map_chr_addr(addr);
            if chr_addr < self.chr.len() {
                self.chr[chr_addr] = data;
            }
        }
    }
    
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    
    fn irq_triggered(&self) -> bool {
        self.irq_pending
    }
    
    fn acknowledge_irq(&mut self) {
        self.irq_pending = false;
    }
    
    fn notify_scanline(&mut self) {
        // Clock IRQ counter on each scanline
        if self.irq_reload {
            self.irq_counter = self.irq_latch;
            self.irq_reload = false;
        } else if self.irq_counter == 0 {
            self.irq_counter = self.irq_latch;
        } else {
            self.irq_counter -= 1;
        }
        
        if self.irq_counter == 0 && self.irq_enabled {
            self.irq_pending = true;
        }
    }
    
    fn reset(&mut self) {
        self.bank_select = 0;
        self.prg_mode = 0;
        self.chr_mode = 0;
        self.bank_registers = [0; 8];
        self.irq_counter = 0;
        self.irq_latch = 0;
        self.irq_enabled = false;
        self.irq_pending = false;
        self.irq_reload = false;
        self.prg_ram_protect = [false, false];
    }
}

impl CartridgeTrait for Mapper004 {
    fn load_ram(&mut self, data: &[u8]) {
        if !data.is_empty() && data.len() <= self.prg_ram.len() {
            self.prg_ram[..data.len()].copy_from_slice(data);
        }
    }
}