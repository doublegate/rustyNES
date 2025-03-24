//! Background rendering for the PPU
//!
//! This module handles rendering the background tiles for the NES.
//! The background is composed of 8x8 pixel tiles arranged in a 32x30 grid (nametable).

use crate::memory::MemoryBus;

#[derive(Clone)]
/// Background rendering state
pub struct Background {
    /// Tile shifter for pattern table low bits
    pub shifter_pattern_lo: u16,
    
    /// Tile shifter for pattern table high bits
    pub shifter_pattern_hi: u16,
    
    /// Attribute table shifter for palette low bits
    pub shifter_attr_lo: u16,
    
    /// Attribute table shifter for palette high bits
    pub shifter_attr_hi: u16,
    
    /// Next tile ID from nametable
    pub next_tile_id: u8,
    
    /// Next tile attribute from attribute table
    pub next_tile_attr: u8,
    
    /// Next tile pattern low byte
    pub next_pattern_lo: u8,
    
    /// Next tile pattern high byte
    pub next_pattern_hi: u8,
}

impl Background {
    /// Create a new background rendering state
    pub fn new() -> Self {
        Background {
            shifter_pattern_lo: 0,
            shifter_pattern_hi: 0,
            shifter_attr_lo: 0,
            shifter_attr_hi: 0,
            next_tile_id: 0,
            next_tile_attr: 0,
            next_pattern_lo: 0,
            next_pattern_hi: 0,
        }
    }
    
    /// Reset the background rendering state
    pub fn reset(&mut self) {
        self.shifter_pattern_lo = 0;
        self.shifter_pattern_hi = 0;
        self.shifter_attr_lo = 0;
        self.shifter_attr_hi = 0;
        self.next_tile_id = 0;
        self.next_tile_attr = 0;
        self.next_pattern_lo = 0;
        self.next_pattern_hi = 0;
    }
    
    /// Update the background shifters
    pub fn update_shifters(&mut self) {
        // Shift the pattern and attribute shifters
        self.shifter_pattern_lo <<= 1;
        self.shifter_pattern_hi <<= 1;
        self.shifter_attr_lo <<= 1;
        self.shifter_attr_hi <<= 1;
    }
    
    /// Load the background shifters with new tile data
    pub fn load_shifters(&mut self) {
        // Load new data into the shifters
        self.shifter_pattern_lo &= 0xFF00;
        self.shifter_pattern_lo |= self.next_pattern_lo as u16;
        
        self.shifter_pattern_hi &= 0xFF00;
        self.shifter_pattern_hi |= self.next_pattern_hi as u16;
        
        // Set attribute shifters based on palette bits
        self.shifter_attr_lo &= 0xFF00;
        self.shifter_attr_lo |= if (self.next_tile_attr & 0x01) != 0 {
            0xFF
        } else {
            0x00
        };
        
        self.shifter_attr_hi &= 0xFF00;
        self.shifter_attr_hi |= if (self.next_tile_attr & 0x02) != 0 {
            0xFF
        } else {
            0x00
        };
    }
    
    /// Fetch tile data for the background
    pub fn fetch_tile_data(&mut self, v: u16, cycle: u16, rendering_enabled: bool, bus: &mut MemoryBus) {
        if !rendering_enabled {
            return;
        }

        match cycle % 8 {
            1 => {
                // Nametable byte
                let addr = 0x2000 | (v & 0x0FFF);
                self.next_tile_id = bus.read(addr);
            },
            3 => {
                // Attribute table byte
                let addr = 0x23C0 | (v & 0x0C00) | ((v >> 4) & 0x38) | ((v >> 2) & 0x07);
                let attr = bus.read(addr);
                
                // Determine which quadrant of the attribute byte to use
                let shift = ((v >> 4) & 0x04) | (v & 0x02);
                self.next_tile_attr = (attr >> shift) & 0x03;
                
                // Update shifters
                self.update_shifters();
            },
            5 => {
                // Pattern table low byte
                let pattern_addr = ((bus.ppu_registers[0] & 0x10) as u16) << 8 | (self.next_tile_id as u16 * 16) | ((v >> 12) & 0x07) as u16;
                self.next_pattern_lo = bus.read(pattern_addr);
                
                // Update shifters
                self.update_shifters();
            },
            7 => {
                // Pattern table high byte
                let pattern_addr = ((bus.ppu_registers[0] & 0x10) as u16) << 8 | (self.next_tile_id as u16 * 16) | ((v >> 12) & 0x07) as u16 | 0x08;
                self.next_pattern_hi = bus.read(pattern_addr);
                
                // Update shifters
                self.update_shifters();
            },
            0 => {
                // Load the new data into the shifters
                self.load_shifters();
            },
            _ => {}
        }
    }
    
    /// Get the background pixel at the current position
    pub fn get_pixel(&self, _v: u16, x: u8) -> (u8, u8) {
        // Get the pixel value from the shifters
        let mux = 0x8000 >> x;
        
        let pixel_lo = if (self.shifter_pattern_lo & mux) != 0 { 1 } else { 0 };
        let pixel_hi = if (self.shifter_pattern_hi & mux) != 0 { 2 } else { 0 };
        let pixel_val = pixel_hi | pixel_lo;
        
        let palette_lo = if (self.shifter_attr_lo & mux) != 0 { 1 } else { 0 };
        let palette_hi = if (self.shifter_attr_hi & mux) != 0 { 2 } else { 0 };
        let palette = palette_hi | palette_lo;
        
        (palette, pixel_val)
    }
}