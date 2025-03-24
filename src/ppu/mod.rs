//! PPU (Picture Processing Unit) implementation
//!
//! The PPU is responsible for generating the video signal for the NES. It renders
//! the background and sprites, and handles various aspects of the NES graphics system.
//!
//! This implementation focuses on cycle-accurate timing and correct rendering.

mod background;
mod palette;
mod sprites;

use std::cell::RefCell;
use std::rc::Rc;
use serde::{Serialize, Deserialize};

use crate::memory::MemoryBus;
use crate::cartridge::Mirroring;

pub use background::*;
pub use palette::*;
pub use sprites::*;

/// PPU screen width in pixels
pub const SCREEN_WIDTH: u32 = 256;

/// PPU screen height in pixels
pub const SCREEN_HEIGHT: u32 = 240;

/// Total PPU scanlines per frame
pub const SCANLINES_PER_FRAME: u16 = 262;

/// Last visible scanline
pub const LAST_VISIBLE_SCANLINE: u16 = 239;

/// Pre-render scanline
pub const PRE_RENDER_SCANLINE: u16 = 261;

/// Visible cycles per scanline
pub const VISIBLE_CYCLES_PER_SCANLINE: u16 = 256;

/// Total cycles per scanline
pub const CYCLES_PER_SCANLINE: u16 = 341;

/// TV system types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TVSystem {
    /// NTSC (North America, Japan, etc.)
    NTSC,
    
    /// PAL (Europe, Australia, etc.)
    PAL,
    
    /// Dendy (Russian NES clone)
    Dendy,
}

/// Represents the NES PPU (Picture Processing Unit)
#[derive(Clone)]
pub struct PPU {
    /// Current cycle within the scanline (0-340)
    pub cycle: u16,
    
    /// Current scanline (0-261 for NTSC, 0-311 for PAL)
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
    
    /// Background rendering state
    pub bg: Background,
    
    /// Sprite rendering state
    pub sprites: Sprites,
    
    /// TV system
    pub tv_system: TVSystem,
    
    /// Current palette (there are several available palettes)
    pub palette_table: Rc<RefCell<PaletteTable>>,
}

impl PPU {
    /// Create a new PPU instance
    pub fn new(tv_system: TVSystem) -> Self {
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
            bg: Background::new(),
            sprites: Sprites::new(),
            tv_system,
            palette_table: Rc::new(RefCell::new(PaletteTable::new_ntsc())),
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
        self.bg.reset();
        self.sprites.reset();
    }

    /// Run a single PPU cycle
    pub fn step(&mut self, bus: &mut MemoryBus) {
        // Clear frame buffer at the start of a new frame
        if self.scanline == 0 && self.cycle == 0 {
            self.frame_buffer.fill(0);
        }

        // Visible scanlines (0-239)
        if self.scanline <= LAST_VISIBLE_SCANLINE {
            // Visible cycles (0-255)
            if self.cycle <= VISIBLE_CYCLES_PER_SCANLINE {
                // Render current pixel
                let rendering_enabled = (bus.ppu_registers[1] & 0x18) != 0;
                
                if rendering_enabled {
                    // Background rendering
                    let bg_pixel = self.bg.get_pixel(self.v, self.x);
                    
                    // Sprite rendering
                    let sprite_pixel = self.sprites.get_pixel(self.cycle - 1, self.scanline);
                    
                    // Determine final pixel color
                    let (palette_index, _) = self.get_pixel_color(bg_pixel, sprite_pixel);
                    
                    // Convert palette index to RGB
                    let color = self.palette_table.borrow().get_color(self.palette_ram[palette_index as usize]);
                    
                    // Write to frame buffer
                    if self.cycle > 0 && self.scanline < SCREEN_HEIGHT as u16 {
                        let x = (self.cycle - 1) as u32;
                        let y = self.scanline as u32;
                        let index = ((y * SCREEN_WIDTH + x) * 3) as usize;
                        
                        self.frame_buffer[index] = color.0;     // R
                        self.frame_buffer[index + 1] = color.1; // G
                        self.frame_buffer[index + 2] = color.2; // B
                    }
                }
                
                // Fetch background tiles
                if rendering_enabled && self.cycle % 8 == 0 {
                    let v = self.v;
                    let cycle = self.cycle;
                    let rendering_enabled = (bus.ppu_registers[1] & 0x18) != 0;
                    self.bg.fetch_tile_data(v, cycle, rendering_enabled, bus);
                }
                
                // Increment horizontal position
                if rendering_enabled && self.cycle == 256 {
                    self.increment_x();
                }
            }
            
            // End of visible scanline
            if self.cycle == 257 {
                // Sprite evaluation for next scanline
                if (bus.ppu_registers[1] & 0x18) != 0 {
                    self.sprites.evaluate_sprites(self.scanline + 1, &self.oam);
                }
                
                // Reset horizontal position
                if (bus.ppu_registers[1] & 0x18) != 0 {
                    self.v = (self.v & 0x7BE0) | (self.t & 0x041F);
                }
            }
        }
        
        // Pre-render scanline (261)
        if self.scanline == PRE_RENDER_SCANLINE {
            // Clear VBlank, sprite 0 hit, and sprite overflow flags
            if self.cycle == 1 {
                bus.ppu_registers[2] &= 0x1F;
                self.nmi_occurred = false;
                self.sprites.sprite_zero_hit = false;
                self.sprites.sprite_overflow = false;
            }
            
            // Clear secondary OAM
            if self.cycle == 1 {
                self.secondary_oam = [0xFF; 32];
            }
            
            // Copy vertical scroll bits
            if self.cycle >= 280 && self.cycle <= 304 && (bus.ppu_registers[1] & 0x18) != 0 {
                self.v = (self.v & 0x041F) | (self.t & 0x7BE0);
            }
        }
        
        // VBlank scanline (241)
        if self.scanline == 241 && self.cycle == 1 {
            self.nmi_occurred = true;
            
            // Update NMI output based on PPUCTRL register (bit 7)
            self.nmi_output = (bus.ppu_registers[0] & 0x80) != 0;
            
            if self.nmi_output && !self.suppress_vblank {
                bus.ppu_registers[2] |= 0x80; // Set VBlank flag in PPUSTATUS
                bus.set_nmi_pending(true);    // Signal NMI to the memory bus
            }
        }
        
        // Increment cycle and scanline counters
        self.cycle += 1;
        if self.cycle > CYCLES_PER_SCANLINE {
            // Skip last cycle on odd frames (NTSC only)
            if self.tv_system == TVSystem::NTSC && !self.even_frame && self.scanline == 261 && (bus.ppu_registers[1] & 0x18) != 0 {
                self.cycle = 0;
                self.scanline = 0;
                self.even_frame = !self.even_frame;
                self.frame += 1;
            } else {
                self.cycle = 0;
                self.scanline += 1;
                
                if self.scanline >= self.scanlines_per_frame() {
                    self.scanline = 0;
                    self.even_frame = !self.even_frame;
                    self.frame += 1;
                }
            }
        }
    }

    /// Check for and trigger NMI
    pub fn check_nmi(&mut self, bus: &mut MemoryBus) {
        // If NMI is pending and enabled, signal to memory bus
        if self.nmi_occurred && self.nmi_output && !self.suppress_vblank {
            bus.set_nmi_pending(true);
            self.nmi_occurred = false;  // Clear the occurred flag after signaling
        }
    }

    /// Get scanlines per frame based on TV system
    fn scanlines_per_frame(&self) -> u16 {
        match self.tv_system {
            TVSystem::NTSC => 262,
            TVSystem::PAL | TVSystem::Dendy => 312,
        }
    }

    /// Get the current frame buffer
    pub fn get_frame_buffer(&self) -> &[u8] {
        &self.frame_buffer
    }

    /// Read a byte from PPU memory
    pub fn read(&self, addr: u16, bus: &MemoryBus) -> u8 {
        let addr = addr & 0x3FFF; // Mirror down
        
        match addr {
            // Pattern tables (0x0000-0x1FFF)
            0x0000..=0x1FFF => {
                if let Some(cart) = bus.get_cartridge() {
                    cart.borrow().read_chr(addr)
                } else {
                    0
                }
            },
            
            // Nametables (0x2000-0x2FFF)
            0x2000..=0x2FFF => {
                let vram_addr = self.mirror_vram_addr(addr, bus) as usize;
                self.vram[vram_addr]
            },
            
            // Palette RAM (0x3F00-0x3FFF)
            0x3F00..=0x3FFF => {
                let palette_addr = self.mirror_palette_addr(addr) as usize;
                self.palette_ram[palette_addr]
            },
            
            _ => 0
        }
    }

    /// Write a byte to PPU memory
    pub fn write(&mut self, addr: u16, value: u8, bus: &MemoryBus) {
        let addr = addr & 0x3FFF; // Mirror down
        
        match addr {
            // Pattern tables (0x0000-0x1FFF)
            0x0000..=0x1FFF => {
                if let Some(cart) = bus.get_cartridge() {
                    cart.borrow_mut().write_chr(addr, value);
                }
            },
            
            // Nametables (0x2000-0x2FFF)
            0x2000..=0x2FFF => {
                let vram_addr = self.mirror_vram_addr(addr, bus) as usize;
                self.vram[vram_addr] = value;
            },
            
            // Palette RAM (0x3F00-0x3FFF)
            0x3F00..=0x3FFF => {
                let palette_addr = self.mirror_palette_addr(addr) as usize;
                self.palette_ram[palette_addr] = value;
            },
            
            _ => {}
        }
    }

    /// Handle mirroring of VRAM addresses based on the cartridge's mirroring mode
    fn mirror_vram_addr(&self, addr: u16, bus: &MemoryBus) -> u16 {
        let addr = addr & 0x2FFF;
        let mirroring = if let Some(cart) = bus.get_cartridge() {
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

    /// Handle mirroring of palette addresses
    fn mirror_palette_addr(&self, addr: u16) -> u16 {
        let addr = addr & 0x3F1F;
        if addr & 0x0F == 0 {
            // $3F00, $3F10, $3F20, $3F30 are mirrors of each other
            addr & 0x000F
        } else if addr & 0x03 == 0 {
            // $3F04, $3F08, $3F0C, $3F14, $3F18, $3F1C are mirrors
            addr & 0x000F
        } else {
            addr & 0x001F
        }
    }

    /// Increment X scroll (horizontal position)
    fn increment_x(&mut self) {
        if (self.v & 0x001F) == 31 {
            // Coarse X = 0, toggle horizontal nametable
            self.v &= !0x001F;
            self.v ^= 0x0400;
        } else {
            // Increment coarse X
            self.v += 1;
        }
    }

    /// Increment Y scroll (vertical position)
    fn increment_y(&mut self) {
        // Fine Y
        let fine_y = (self.v >> 12) & 0x07;
        if fine_y == 7 {
            // Fine Y = 0, increment coarse Y
            self.v &= !0x7000;
            
            // Coarse Y
            let coarse_y = (self.v >> 5) & 0x1F;
            if coarse_y == 29 {
                // Coarse Y = 0, toggle vertical nametable
                self.v &= !0x03E0;
                self.v ^= 0x0800;
            } else if coarse_y == 31 {
                // Coarse Y = 0, no nametable toggle
                self.v &= !0x03E0;
            } else {
                // Increment coarse Y
                self.v += 0x0020;
            }
        } else {
            // Increment fine Y
            self.v += 0x1000;
        }
    }

    /// Determine the final pixel color by combining background and sprite pixels
    /// This is a performance-critical function, so we've optimized it
    #[inline]
    fn get_pixel_color(&mut self, bg_pixel: (u8, u8), sprite_pixel: (u8, u8, bool, bool)) -> (u8, bool) {
        let (bg_palette, bg_pixel_value) = bg_pixel;
        let (sprite_palette, sprite_pixel_value, sprite_priority, sprite_zero) = sprite_pixel;
        
        // Check for sprite zero hit (optimize this check)
        if bg_pixel_value != 0 && sprite_pixel_value != 0 && sprite_zero && self.cycle != 255 {
            self.sprites.sprite_zero_hit = true;
        }
        
        // Using if-else instead of match for better performance
        if bg_pixel_value == 0 {
            if sprite_pixel_value == 0 {
                // Both transparent, show universal background color
                (0, false)
            } else {
                // Background transparent, sprite visible
                (0x10 | (sprite_palette << 2) | sprite_pixel_value, false)
            }
        } else {
            if sprite_pixel_value == 0 {
                // Sprite transparent, background visible
                ((bg_palette << 2) | bg_pixel_value, false)
            } else if sprite_priority {
                // Sprite behind background
                ((bg_palette << 2) | bg_pixel_value, true)
            } else {
                // Sprite in front of background
                (0x10 | (sprite_palette << 2) | sprite_pixel_value, false)
            }
        }
    }
}