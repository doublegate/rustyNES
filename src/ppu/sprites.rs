//! Sprite rendering for the PPU
//!
//! This module handles rendering the sprite tiles for the NES.
//! Sprites are 8x8 or 8x16 pixel objects that can be positioned anywhere on screen.

/// Maximum number of sprites per scanline
pub const MAX_SPRITES_PER_SCANLINE: usize = 8;

/// Sprite rendering state
#[derive(Clone)]
pub struct Sprites {
    /// Sprite zero is present on current scanline
    pub sprite_zero_present: bool,
    
    /// Sprite zero hit flag
    pub sprite_zero_hit: bool,
    
    /// Sprite overflow flag
    pub sprite_overflow: bool,
    
    /// Sprite scanline buffer
    scanline_sprites: [SpriteData; MAX_SPRITES_PER_SCANLINE],
    
    /// Number of sprites on current scanline
    sprite_count: usize,
}

/// Data for a single sprite
#[derive(Copy, Clone, Default)]
struct SpriteData {
    /// X position
    x: u8,
    
    /// Y position
    y: u8,
    
    /// Tile index
    tile: u8,
    
    /// Attribute byte
    attribute: u8,
    
    /// Pattern data for sprite (low byte)
    pattern_lo: u8,
    
    /// Pattern data for sprite (high byte)
    pattern_hi: u8,
}

impl SpriteData {
    /// Create a new sprite data
    fn new(x: u8, y: u8, tile: u8, attribute: u8) -> Self {
        SpriteData {
            x,
            y,
            tile,
            attribute,
            pattern_lo: 0,
            pattern_hi: 0,
        }
    }
}

impl Sprites {
    /// Create a new sprite rendering state
    pub fn new() -> Self {
        Sprites {
            sprite_zero_present: false,
            sprite_zero_hit: false,
            sprite_overflow: false,
            scanline_sprites: [SpriteData::default(); MAX_SPRITES_PER_SCANLINE],
            sprite_count: 0,
        }
    }
    
    /// Reset the sprite rendering state
    pub fn reset(&mut self) {
        self.sprite_zero_present = false;
        self.sprite_zero_hit = false;
        self.sprite_overflow = false;
        self.scanline_sprites = [SpriteData::default(); MAX_SPRITES_PER_SCANLINE];
        self.sprite_count = 0;
    }
    
    /// Evaluate sprites for the next scanline
    pub fn evaluate_sprites(&mut self, scanline: u16, oam: &[u8]) {
        // Clear sprite count
        self.sprite_count = 0;
        self.sprite_zero_present = false;
        
        // Check which sprites are visible on the next scanline
        for i in 0..64 {
            // Get sprite data
            let idx = i * 4;
            let y = oam[idx];
            let tile = oam[idx + 1];
            let attr = oam[idx + 2];
            let x = oam[idx + 3];
            
            // Check if sprite is visible on this scanline
            let in_range = scanline >= y as u16 && scanline < (y as u16 + 8);
            
            if in_range {
                // Add sprite to scanline buffer
                if self.sprite_count < MAX_SPRITES_PER_SCANLINE {
                    self.scanline_sprites[self.sprite_count] = SpriteData::new(x, y, tile, attr);
                    
                    // Check if this is sprite zero
                    if i == 0 {
                        self.sprite_zero_present = true;
                    }
                    
                    self.sprite_count += 1;
                } else {
                    // Sprite overflow
                    self.sprite_overflow = true;
                    break;
                }
            }
        }
    }
    
    /// Load pattern data for sprites
    pub fn load_sprite_patterns(&mut self, ppu_ctrl: u8, pattern_table: &[u8]) {
        // Pattern table selection for sprites
        let sprite_pattern_table_addr = if (ppu_ctrl & 0x08) != 0 { 0x1000 } else { 0x0000 };
        
        // Load pattern data for each sprite
        for i in 0..self.sprite_count {
            let sprite = &mut self.scanline_sprites[i];
            
            // Determine pattern address
            let mut pattern_addr = sprite_pattern_table_addr + (sprite.tile as u16 * 16);
            
            // Apply Y flipping if needed
            let row = if (sprite.attribute & 0x80) != 0 {
                7 - (sprite.y as u16 % 8)
            } else {
                sprite.y as u16 % 8
            };
            
            pattern_addr += row;
            
            // Load pattern data
            sprite.pattern_lo = pattern_table[pattern_addr as usize];
            sprite.pattern_hi = pattern_table[(pattern_addr + 8) as usize];
            
            // Apply X flipping if needed
            if (sprite.attribute & 0x40) != 0 {
                // Flip bits horizontally
                sprite.pattern_lo = Self::flip_byte(sprite.pattern_lo);
                sprite.pattern_hi = Self::flip_byte(sprite.pattern_hi);
            }
        }
    }
    
    /// Get the sprite pixel at the given position
    pub fn get_pixel(&self, x: u16, _y: u16) -> (u8, u8, bool, bool) {
        // Check each sprite in the secondary OAM
        for i in 0..8 {
            let sprite_x = self.scanline_sprites[i].x;
            let sprite_attr = self.scanline_sprites[i].attribute;
            let sprite_pattern_lo = self.scanline_sprites[i].pattern_lo;
            let sprite_pattern_hi = self.scanline_sprites[i].pattern_hi;
            
            // Check if sprite is visible at this x position
            if x >= sprite_x as u16 && x < (sprite_x as u16 + 8) {
                let mut pattern_bit = 7 - ((x - sprite_x as u16) as u8);
                
                // Handle horizontal flip
                if (sprite_attr & 0x40) != 0 {
                    pattern_bit = 7 - pattern_bit;
                }
                
                // Get pixel value
                let pixel_lo = ((sprite_pattern_lo >> pattern_bit) & 0x01) as u8;
                let pixel_hi = ((sprite_pattern_hi >> pattern_bit) & 0x01) << 1;
                let pixel_val = pixel_hi | pixel_lo;
                
                // If pixel is non-transparent
                if pixel_val != 0 {
                    let palette = (sprite_attr & 0x03) as u8;
                    let behind_bg = (sprite_attr & 0x20) != 0;
                    let sprite_zero = i == 0;
                    
                    return (palette, pixel_val, behind_bg, sprite_zero);
                }
            }
        }
        
        // No sprite pixel found
        (0, 0, false, false)
    }
    
    /// Flip a byte (reverse bit order)
    fn flip_byte(b: u8) -> u8 {
        let mut result = 0;
        for i in 0..8 {
            result |= ((b >> i) & 0x01) << (7 - i);
        }
        result
    }
}