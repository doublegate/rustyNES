//! Utility functions and helpers for the emulator
//!
//! This module contains various utility functions and helpers used throughout the
//! emulator code.

/// Combine two 8-bit values into a 16-bit value (little-endian)
#[inline]
pub fn combine_bytes(low: u8, high: u8) -> u16 {
    u16::from_le_bytes([low, high])
}

/// Split a 16-bit value into two 8-bit values (little-endian)
#[inline]
pub fn split_bytes(value: u16) -> (u8, u8) {
    let [low, high] = value.to_le_bytes();
    (low, high)
}

/// Determine if a page boundary is crossed when adding an offset to an address
#[inline]
pub fn page_boundary_crossed(addr: u16, offset: u8) -> bool {
    (addr & 0xFF00) != ((addr + offset as u16) & 0xFF00)
}

/// Get the stack pointer address
#[inline]
pub fn stack_address(sp: u8) -> u16 {
    0x0100 + sp as u16
}

/// Debug hexdump of a memory region
pub fn hexdump(data: &[u8], start_addr: u16) {
    for (i, chunk) in data.chunks(16).enumerate() {
        let addr = start_addr + (i * 16) as u16;
        print!("{:04X}: ", addr);
        
        for (j, byte) in chunk.iter().enumerate() {
            print!("{:02X} ", byte);
            if j == 7 {
                print!(" ");
            }
        }
        
        // Padding for incomplete lines
        for _ in chunk.len()..16 {
            print!("   ");
        }
        if chunk.len() <= 8 {
            print!(" ");
        }
        
        print!(" |");
        for byte in chunk {
            if *byte >= 0x20 && *byte < 0x7F {
                print!("{}", *byte as char);
            } else {
                print!(".");
            }
        }
        println!("|");
    }
}

/// Convert a byte to a binary string
pub fn byte_to_binary(byte: u8) -> String {
    format!("{:08b}", byte)
}

/// Format a 16-bit address as a hex string
pub fn format_addr(addr: u16) -> String {
    format!("${:04X}", addr)
}

/// Format an 8-bit value as a hex string
pub fn format_byte(value: u8) -> String {
    format!("${:02X}", value)
}

/// Calculate the number of CPU cycles for a given PPU scanline
pub fn cpu_cycles_per_scanline(tv_system: crate::ppu::TVSystem) -> u32 {
    match tv_system {
        crate::ppu::TVSystem::NTSC => 113, // 341 PPU cycles / 3 = ~113.67 CPU cycles
        crate::ppu::TVSystem::PAL | crate::ppu::TVSystem::Dendy => 106, // 319.5 PPU cycles / 3 = ~106.5 CPU cycles
    }
}

/// Check if a bit is set in a byte
#[inline]
pub fn check_bit(value: u8, bit: u8) -> bool {
    (value & (1 << bit)) != 0
}

/// Set a bit in a byte
#[inline]
pub fn set_bit(value: &mut u8, bit: u8) {
    *value |= 1 << bit;
}

/// Clear a bit in a byte
#[inline]
pub fn clear_bit(value: &mut u8, bit: u8) {
    *value &= !(1 << bit);
}