//! PPU (Picture Processing Unit) implementation
//!
//! The PPU is responsible for generating the video signal for the NES. It renders
//! the background and sprites, and handles various aspects of the NES graphics system.
//!
//! This implementation provides cycle-accurate timing to ensure proper synchronization
//! with the CPU and other components.

// use log::{debug, trace};

use crate::memory::MemoryBus;

/// PPU screen width in pixels
pub const SCREEN_WIDTH: u32 = 256;

/// PPU screen height in pixels
pub const SCREEN_HEIGHT: u32 = 240;

/// NES palette (based on FCEUX palette)
#[rustfmt::skip]
const NES_PALETTE: [(u8, u8, u8); 64] = [
    (0x74, 0x74, 0x74), (0x24, 0x18, 0x8C), (0x00, 0x00, 0xA8), (0x44, 0x00, 0x9C),
    (0x8C, 0x00, 0x74), (0xA8, 0x00, 0x10), (0xA4, 0x00, 0x00), (0x7C, 0x08, 0x00),
    (0x40, 0x2C, 0x00), (0x00, 0x44, 0x00), (0x00, 0x50, 0x00), (0x00, 0x3C, 0x14),
    (0x18, 0x3C, 0x5C), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00),
    
    (0xBC, 0xBC, 0xBC), (0x00, 0x70, 0xEC), (0x20, 0x38, 0xEC), (0x80, 0x00, 0xF0),
    (0xBC, 0x00, 0xBC), (0xE4, 0x00, 0x58), (0xD8, 0x28, 0x00), (0xC8, 0x4C, 0x0C),
    (0x88, 0x70, 0x00), (0x00, 0x94, 0x00), (0x00, 0xA8, 0x00), (0x00, 0x90, 0x38),
    (0x00, 0x80, 0x88), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00),
    
    (0xFC, 0xFC, 0xFC), (0x3C, 0xBC, 0xFC), (0x5C, 0x94, 0xFC), (0xCC, 0x88, 0xFC),
    (0xF4, 0x78, 0xFC), (0xFC, 0x74, 0xB4), (0xFC, 0x74, 0x60), (0xFC, 0x98, 0x38),
    (0xF0, 0xBC, 0x3C), (0x80, 0xD0, 0x10), (0x4C, 0xDC, 0x48), (0x58, 0xF8, 0x98),
    (0x00, 0xE8, 0xD8), (0x78, 0x78, 0x78), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00),
    
    (0xFC, 0xFC, 0xFC), (0xA8, 0xE4, 0xFC), (0xC4, 0xD4, 0xFC), (0xD4, 0xC8, 0xFC),
    (0xFC, 0xC4, 0xFC), (0xFC, 0xC4, 0xD8), (0xFC, 0xBC, 0xB0), (0xFC, 0xD8, 0xA8),
    (0xFC, 0xE4, 0xA0), (0xE0, 0xFC, 0xA0), (0xA8, 0xF0, 0xBC), (0xB0, 0xFC, 0xCC),
    (0x9C, 0xFC, 0xF0), (0xC4, 0xC4, 0xC4), (0x00, 0x00, 0x00), (0x00, 0x00, 0x00),
];

/// Represents the NES PPU (Picture Processing Unit)
pub struct PPU {
    /// Current cycle within the scanline (0-340)
    pub cycle: u16,
    
    /// Current scanline (0-261, with 0-239 being visible, 240 being post-render, 241-260 being vblank, and 261 being pre-render)
    pub scanline: u16,
    
    /// Current frame count
    pub frame: u64,
    
    /// PPU internal VRAM address (current)
    pub v: u16,
    
    /// PPU internal VRAM address (temporary)
    pub t: u16,
    
    /// Fine X scroll (3 bits)
    pub x: u8,
    
    /// First or second write toggle
    pub w: bool,
    
    /// NMI occurred flag
    pub nmi_occurred: bool,
    
    /// NMI output flag
    pub nmi_output: bool,
    
    /// Even or odd frame
    pub even_frame: bool,
    
    /// PPU internal data buffer
    pub data_buffer: u8,
    
    /// VBlank suppression flag
    pub suppress_vblank: bool,
    
    /// OAM (Object Attribute Memory) - 256 bytes for sprite data
    pub oam: [u8; 256],
    
    /// Secondary OAM - 32 bytes for sprite data for the current scanline
    pub secondary_oam: [u8; 32],
    
    /// PPU internal VRAM - 2KB
    pub vram: [u8; 2048],
    
    /// Palette RAM - 32 bytes
    pub palette_ram: [u8; 32],
    
    /// Frame buffer (RGB24 format)
    pub frame_buffer: Vec<u8>,
    
    /// Current render buffer (RGB24 format)
    pub render_buffer: Vec<u8>,
    
    /// Sprite zero hit possible flag
    pub sprite_zero_hit_possible: bool,
    
    /// Sprite zero being rendered flag
    pub sprite_zero_being_rendered: bool,
    
    // Internal rendering state
    
    /// Tile shifter pattern table low bits
    bg_shifter_pattern_lo: u16,
    
    /// Tile shifter pattern table high bits
    bg_shifter_pattern_hi: u16,
    
    /// Tile shifter attribute table low bits
    bg_shifter_attr_lo: u16,
    
    /// Tile shifter attribute table high bits
    bg_shifter_attr_hi: u16,
    
    /// Next tile ID
    next_tile_id: u8,
    
    /// Next tile attribute
    next_tile_attr: u8,
    
    /// Next tile low pattern
    next_tile_lsb: u8,
    
    /// Next tile high pattern
    next_tile_msb: u8,
}

impl PPU {
    /// Create a new PPU instance
    pub fn new() -> Self {
        PPU {
            cycle: 0,
            scanline: 0,
            frame: 0,
            v: 0,
            t: 0,
            x: 0,
            w: false,
            nmi_occurred: false,
            nmi_output: false,
            even_frame: true,
            data_buffer: 0,
            suppress_vblank: false,
            oam: [0; 256],
            secondary_oam: [0; 32],
            vram: [0; 2048],
            palette_ram: [0; 32],
            frame_buffer: vec![0; (SCREEN_WIDTH * SCREEN_HEIGHT * 3) as usize],
            render_buffer: vec![0; (SCREEN_WIDTH * SCREEN_HEIGHT * 3) as usize],
            sprite_zero_hit_possible: false,
            sprite_zero_being_rendered: false,
            
            // Rendering state
            bg_shifter_pattern_lo: 0,
            bg_shifter_pattern_hi: 0,
            bg_shifter_attr_lo: 0,
            bg_shifter_attr_hi: 0,
            next_tile_id: 0,
            next_tile_attr: 0,
            next_tile_lsb: 0,
            next_tile_msb: 0,
        }
    }

    /// Reset the PPU
    pub fn reset(&mut self) {
        self.cycle = 0;
        self.scanline = 0;
        self.frame = 0;
        self.v = 0;
        self.t = 0;
        self.x = 0;
        self.w = false;
        self.nmi_occurred = false;
        self.nmi_output = false;
        self.even_frame = true;
        self.data_buffer = 0;
        self.suppress_vblank = false;
        
        // Clear OAM, VRAM, and palette RAM
        self.oam = [0; 256];
        self.secondary_oam = [0; 32];
        self.vram = [0; 2048];
        self.palette_ram = [0; 32];
        
        // Reset rendering state
        self.bg_shifter_pattern_lo = 0;
        self.bg_shifter_pattern_hi = 0;
        self.bg_shifter_attr_lo = 0;
        self.bg_shifter_attr_hi = 0;
        self.next_tile_id = 0;
        self.next_tile_attr = 0;
        self.next_tile_lsb = 0;
        self.next_tile_msb = 0;
    }

    /// Run a single PPU cycle
    pub fn step(&mut self, bus: &mut MemoryBus) {
        // Handle pre-render scanline (261)
        if self.scanline == 261 {
            if self.cycle == 1 {
                // Clear VBlank, sprite 0 hit, and sprite overflow flags
                bus.ppu_registers[2] &= 0x1F;
                self.nmi_occurred = false;
            }
        }
        
        // Handle visible scanlines (0-239)
        if self.scanline < 240 {
            // Visible scanline cycle handling
            // In a complete implementation, this would include:
            // - Background rendering
            // - Sprite evaluation
            // - Pixel rendering
            
            // Simplified rendering logic
            if self.cycle > 0 && self.cycle <= 256 && self.scanline < SCREEN_HEIGHT as u16 {
                let x = (self.cycle - 1) as u32;
                let y = self.scanline as u32;
                let index = ((y * SCREEN_WIDTH + x) * 3) as usize;
                
                // For now, just render a test pattern
                let color_index = ((x / 16) % 16 + (y / 16) % 16 * 16) % 64;
                let (r, g, b) = NES_PALETTE[color_index as usize];
                
                self.render_buffer[index] = r;
                self.render_buffer[index + 1] = g;
                self.render_buffer[index + 2] = b;
            }
        }
        
        // Handle VBlank start
        if self.scanline == 241 && self.cycle == 1 {
            // Set VBlank flag
            bus.ppu_registers[2] |= 0x80;
            self.nmi_occurred = true;
            
            // Trigger NMI if enabled
            if (bus.ppu_registers[0] & 0x80) != 0 {
                bus.ppu_registers[2] |= 0x80;
                // Copy render buffer to frame buffer
                self.frame_buffer.copy_from_slice(&self.render_buffer);
            }
        }
        
        // Update cycle and scanline counters
        self.cycle += 1;
        if self.cycle > 340 {
            self.cycle = 0;
            self.scanline += 1;
            if self.scanline > 261 {
                self.scanline = 0;
                self.frame += 1;
                self.even_frame = !self.even_frame;
            }
        }
    }

    /// Get the current frame buffer
    pub fn get_frame_buffer(&self) -> &[u8] {
        &self.frame_buffer
    }

    /// Read from the PPU's VRAM address space
    pub fn read(&mut self, addr: u16) -> u8 {
        let addr = addr & 0x3FFF; // Mirror down
        
        match addr {
            // Pattern tables (0x0000-0x1FFF) - CHR ROM/RAM
            0x0000..=0x1FFF => {
                // In a complete implementation, this would read from the cartridge's CHR ROM/RAM
                0
            },
            
            // Nametables (0x2000-0x2FFF) - VRAM
            0x2000..=0x2FFF => {
                let vram_addr = self.mirror_vram_address(addr) as usize;
                self.vram[vram_addr]
            },
            
            // Palette RAM (0x3F00-0x3FFF)
            0x3F00..=0x3FFF => {
                let palette_addr = self.mirror_palette_address(addr) as usize;
                self.palette_ram[palette_addr]
            },
            
            _ => 0,
        }
    }

    /// Write to the PPU's VRAM address space
    pub fn write(&mut self, addr: u16, value: u8) {
        let addr = addr & 0x3FFF; // Mirror down
        
        match addr {
            // Pattern tables (0x0000-0x1FFF) - CHR ROM/RAM
            0x0000..=0x1FFF => {
                // In a complete implementation, this would write to the cartridge's CHR ROM/RAM
                // if it's actually RAM
            },
            
            // Nametables (0x2000-0x2FFF) - VRAM
            0x2000..=0x2FFF => {
                let vram_addr = self.mirror_vram_address(addr) as usize;
                self.vram[vram_addr] = value;
            },
            
            // Palette RAM (0x3F00-0x3FFF)
            0x3F00..=0x3FFF => {
                let palette_addr = self.mirror_palette_address(addr) as usize;
                self.palette_ram[palette_addr] = value;
            },
            
            _ => {}
        }
    }

    /// Handle mirroring of VRAM addresses based on the cartridge's mirroring mode
    fn mirror_vram_address(&self, addr: u16) -> u16 {
        // Simplified mirroring - horizontal mirroring
        // In a complete implementation, this would depend on the cartridge's mirroring mode
        let addr = addr & 0x2FFF;
        let offset = addr & 0x03FF;
        let mirror = (addr & 0x0C00) >> 10;
        
        // Horizontal mirroring
        let table = match mirror {
            0 => 0, // Nametable 0
            1 => 0, // Mirror of nametable 0
            2 => 1, // Nametable 1
            3 => 1, // Mirror of nametable 1
            _ => unreachable!(),
        };
        
        (table << 10) | offset
    }

    /// Handle mirroring of palette addresses
    fn mirror_palette_address(&self, addr: u16) -> u16 {
        let addr = addr & 0x3F1F;
        match addr {
            0x3F10 => 0x3F00,
            0x3F14 => 0x3F04,
            0x3F18 => 0x3F08,
            0x3F1C => 0x3F0C,
            _ => addr & 0x1F,
        }
    }
}