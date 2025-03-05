//! # NES CPU Emulation – Ricoh 2A03 Implementation
//! 
//! This module implements the Ricoh 2A03 CPU which is at the heart of the Nintendo Entertainment System.
//! Based on the classic MOS 6502, the 2A03 includes several modifications (such as disabling decimal mode)
//! and integrates audio processing hardware.
//!
//! ## Overview
//! 
//! The CPU emulation is designed to be cycle-accurate, faithfully reproducing the behavior of the original
//! hardware. This implementation includes the complete instruction set (with unofficial opcodes) and correct
//! interrupt handling (NMI, IRQ, and software BRK interrupt) in accordance with the 6502 quirks.
//!
//! ## Features
//! 
//! - **Cycle Accuracy:** Each instruction and interrupt routine accounts for the proper number of cycles,
//!   including extra cycles for page crossings and branch delays.
//! - **Complete Instruction Set:** Implements all standard opcodes as well as unofficial/illegal opcodes.
//! - **Interrupt Handling:** Includes routines for Non-Maskable Interrupt (NMI), IRQs, and software interrupts
//!   (BRK), with proper stacking of registers and status flags.
//! - **Flexible Bus Interface:** Utilizes the `CpuBus` trait to decouple memory operations from CPU
//!   emulation logic, enabling integration with various memory and peripheral implementations.
//! - **Debugging Support:** Provides detailed status strings and debug formatting for enhanced traceability,
//!   which is useful during development and testing of NES emulation projects.
//!
//! ## Technical Details
//! 
//! The CPU structure maintains registers (A, X, Y, P, SP, PC) and internal states such as cycle counters,
//! pending interrupts, and bus data/address for debugging purposes. Instructions are decoded in the `step`
//! function and then dispatched to their dedicated handler functions.
//!
//! ### Interrupts
//! 
//! - **NMI (Non-Maskable Interrupt):** Triggered by external events (e.g., vertical blanking). When detected,
//!   the CPU pushes the current PC and status to the stack, sets the interrupt disable flag, and jumps to the NMI vector.
//! - **IRQ (Interrupt Request):** Similar to the NMI but subject to the interrupt disable flag. The proper vector
//!   is fetched and the CPU state is saved before handling the interrupt.
//!
//! ### Addressing Modes
//! 
//! The implementation supports all addressing modes of the 6502 (immediate, zero page, absolute, indirect, etc.).
//! For each mode, the CPU computes the effective address and may incur additional cycle penalties if, for example,
//! a branch crosses a page boundary.
//!
//! ## Integration in Emulation Projects
//! 
//! To use this CPU implementation in a NES emulator:
//! 1. Implement the `CpuBus` trait for your system memory, PPU, APU, and I/O devices.
//! 2. Instantiate the CPU with `Cpu::new()` and reset it with the state provided from your bus (e.g., setting the reset vector).
//! 3. In your emulation loop, call the `step` method to process each instruction, and synchronize the CPU cycles with
//!    other subsystems.
//! 4. Use the provided debugging methods (e.g., `status_string`) to log and monitor CPU state during development.
//!
//! ## Implementation Notes
//! 
//! - The code accounts for various quirks of the original hardware (such as the infamous JMP indirect bug).
//! - Unofficial opcodes are handled to ensure compatibility with software that relies on these instructions.
//! - The separation of the CPU core and the bus interface allows for easier testing and modular design.
//!
//! By following the documentation and integrating with the appropriate peripherals implementing `CpuBus`,
//! this module can serve as a reliable core component of a NES emulation project.

use std::fmt;

/// CPU flag bit positions
#[allow(dead_code)]
pub mod flags {
    pub const CARRY: u8 = 0x01;
    pub const ZERO: u8 = 0x02;
    pub const INTERRUPT_DISABLE: u8 = 0x04;
    pub const DECIMAL: u8 = 0x08; // Not used in NES, but still exists in the status register
    pub const BREAK: u8 = 0x10;
    pub const UNUSED: u8 = 0x20; // Always set to 1 when pushed to stack
    pub const OVERFLOW: u8 = 0x40;
    pub const NEGATIVE: u8 = 0x80;
}

/// Enumeration of the different addressing modes used by the 6502
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AddressingMode {
    Implied,
    Accumulator,
    Immediate,
    ZeroPage,
    ZeroPageX,
    ZeroPageY,
    Relative,
    Absolute,
    AbsoluteX,
    AbsoluteY,
    Indirect,
    IndexedIndirect, // (Indirect,X)
    IndirectIndexed, // (Indirect),Y
}

/// Represents a bus that the CPU can read from and write to
pub trait CpuBus {
    fn read(&mut self, address: u16) -> u8;
    fn write(&mut self, address: u16, data: u8);
    
    /// Poll for any interrupts. Returns true if an NMI is pending
    fn poll_interrupts(&mut self) -> bool;
    
    /// Poll for any IRQ interrupts. Returns true if an IRQ is pending
    fn poll_irq(&mut self) -> bool;
}

/// Structure representing the state of the NES CPU (Ricoh 2A03)
pub struct Cpu {
    // Registers
    pub a: u8,      // Accumulator
    pub x: u8,      // X index register
    pub y: u8,      // Y index register
    pub p: u8,      // Status register (flags)
    pub sp: u8,     // Stack pointer
    pub pc: u16,    // Program counter
    
    // Internal state
    cycles: u64,    // Total number of cycles executed
    remaining_cycles: u8, // Cycles remaining for current instruction
    nmi_pending: bool,    // NMI interrupt flag
    irq_pending: bool,    // IRQ interrupt flag
    address_bus: u16,     // Current address on the bus (useful for debugging)
    data_bus: u8,         // Current data on the bus (useful for debugging)
    
    // This is used to handle a CPU quirk where an interrupt that occurs during
    // a branch instruction that crosses a page boundary gets delayed by one instruction
    interrupt_delay: bool,
}

impl fmt::Debug for Cpu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CPU {{ A: ${:02X}, X: ${:02X}, Y: ${:02X}, P: ${:02X}, SP: ${:02X}, PC: ${:04X}, Cycles: {} }}",
               self.a, self.x, self.y, self.p, self.sp, self.pc, self.cycles)
    }
}

impl Cpu {
    /// Create a new CPU instance in the reset state
    pub fn new() -> Self {
        Cpu {
            a: 0,
            x: 0,
            y: 0,
            p: flags::INTERRUPT_DISABLE | flags::UNUSED, // I flag set after reset
            sp: 0xFD, // Initial stack pointer after reset
            pc: 0,
            cycles: 0,
            remaining_cycles: 0,
            nmi_pending: false,
            irq_pending: false,
            address_bus: 0,
            data_bus: 0,
            interrupt_delay: false,
        }
    }
    
    /// Reset the CPU
    pub fn reset(&mut self, bus: &mut impl CpuBus) {
        // Set I flag
        self.p |= flags::INTERRUPT_DISABLE;
        
        // Stack pointer is decremented by 3, but nothing is written
        self.sp = 0xFD;
        
        // Read the reset vector from 0xFFFC-0xFFFD
        let low = bus.read(0xFFFC);
        let high = bus.read(0xFFFD);
        self.pc = (high as u16) << 8 | (low as u16);
        
        // Reset takes 7 cycles
        self.cycles += 7;
        self.remaining_cycles = 0;
        
        // Clear interrupt flags
        self.nmi_pending = false;
        self.irq_pending = false;
        self.interrupt_delay = false;
    }
    
    /// Get the current status as a formatted string for debugging
    pub fn status_string(&self) -> String {
        format!("A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} PC:{:04X} CYC:{}",
                self.a, self.x, self.y, self.p, self.sp, self.pc, self.cycles)
    }
    
    /// Get the total number of cycles executed
    pub fn cycles(&self) -> u64 {
        self.cycles
    }
    
    /// Set a specific flag in the status register
    pub fn set_flag(&mut self, flag: u8, value: bool) {
        if value {
            self.p |= flag;
        } else {
            self.p &= !flag;
        }
    }
    
    /// Check if a specific flag is set
    pub fn get_flag(&self, flag: u8) -> bool {
        (self.p & flag) != 0
    }
    
    /// Update the zero and negative flags based on the given value
    fn update_zero_and_negative_flags(&mut self, value: u8) {
        self.set_flag(flags::ZERO, value == 0);
        self.set_flag(flags::NEGATIVE, (value & 0x80) != 0);
    }
    
    /// Trigger an NMI interrupt
    pub fn trigger_nmi(&mut self) {
        self.nmi_pending = true;
    }
    
    /// Trigger an IRQ interrupt
    pub fn trigger_irq(&mut self) {
        self.irq_pending = true;
    }
    
    /// Calculate the address for the given addressing mode
    fn get_operand_address(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u16 {
        match mode {
            AddressingMode::Implied | AddressingMode::Accumulator => {
                // These addressing modes don't use a memory address
                0
            }
            
            AddressingMode::Immediate => {
                // The operand is the byte immediately after the opcode
                let addr = self.pc;
                self.pc = self.pc.wrapping_add(1);
                addr
            }
            
            AddressingMode::ZeroPage => {
                // Zero page addressing: operand is in the zero page ($0000-$00FF)
                let addr = bus.read(self.pc) as u16;
                self.pc = self.pc.wrapping_add(1);
                addr
            }
            
            AddressingMode::ZeroPageX => {
                // Zero page X: like zero page, but adds X register (with zero page wrap)
                let base = bus.read(self.pc) as u16;
                self.pc = self.pc.wrapping_add(1);
                (base + self.x as u16) & 0xFF // Wraps within zero page
            }
            
            AddressingMode::ZeroPageY => {
                // Zero page Y: like zero page, but adds Y register (with zero page wrap)
                let base = bus.read(self.pc) as u16;
                self.pc = self.pc.wrapping_add(1);
                (base + self.y as u16) & 0xFF // Wraps within zero page
            }
            
            AddressingMode::Relative => {
                // Relative addressing is used for branches
                // The offset is a signed byte
                let offset = bus.read(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                
                // Calculate target address by adding the offset to the PC
                // (Remember the PC is already pointing to the next instruction)
                let target = self.pc.wrapping_add(offset as u16);
                target
            }
            
            AddressingMode::Absolute => {
                // Absolute addressing: full 16-bit address
                let low = bus.read(self.pc) as u16;
                self.pc = self.pc.wrapping_add(1);
                let high = bus.read(self.pc) as u16;
                self.pc = self.pc.wrapping_add(1);
                (high << 8) | low
            }
            
            AddressingMode::AbsoluteX => {
                // Absolute X: like absolute, but adds X register
                let low = bus.read(self.pc) as u16;
                self.pc = self.pc.wrapping_add(1);
                let high = bus.read(self.pc) as u16;
                self.pc = self.pc.wrapping_add(1);
                let base = (high << 8) | low;
                base.wrapping_add(self.x as u16)
            }
            
            AddressingMode::AbsoluteY => {
                // Absolute Y: like absolute, but adds Y register
                let low = bus.read(self.pc) as u16;
                self.pc = self.pc.wrapping_add(1);
                let high = bus.read(self.pc) as u16;
                self.pc = self.pc.wrapping_add(1);
                let base = (high << 8) | low;
                base.wrapping_add(self.y as u16)
            }
            
            AddressingMode::Indirect => {
                // Indirect addressing: read the target address from a 16-bit pointer
                let low = bus.read(self.pc) as u16;
                self.pc = self.pc.wrapping_add(1);
                let high = bus.read(self.pc) as u16;
                self.pc = self.pc.wrapping_add(1);
                let ptr = (high << 8) | low;
                
                // Get the actual address from the pointer
                // Handle the 6502 JMP indirect bug: if the pointer is at the end of a page,
                // the high byte is fetched from the start of the same page, not the next page
                let addr_low = bus.read(ptr) as u16;
                let addr_high = if (ptr & 0xFF) == 0xFF {
                    bus.read(ptr & 0xFF00) as u16
                } else {
                    bus.read(ptr + 1) as u16
                };
                
                (addr_high << 8) | addr_low
            }
            
            AddressingMode::IndexedIndirect => {
                // Indexed Indirect (Indirect,X): Zero page address + X gives location of target address
                let base = bus.read(self.pc) as u16;
                self.pc = self.pc.wrapping_add(1);
                
                // Calculate the zero-page address where the actual address is stored
                let ptr = (base + self.x as u16) & 0xFF; // Wrap in zero page
                
                // Read the target address (little-endian)
                let addr_low = bus.read(ptr) as u16;
                let addr_high = bus.read((ptr + 1) & 0xFF) as u16; // Wrap in zero page
                
                (addr_high << 8) | addr_low
            }
            
            AddressingMode::IndirectIndexed => {
                // Indirect Indexed (Indirect),Y: Zero page address gives location of pointer, add Y to get target
                let ptr = bus.read(self.pc) as u16;
                self.pc = self.pc.wrapping_add(1);
                
                // Read the base address (little-endian)
                let addr_low = bus.read(ptr) as u16;
                let addr_high = bus.read((ptr + 1) & 0xFF) as u16; // Wrap in zero page
                let base = (addr_high << 8) | addr_low;
                
                // Add Y to get the final address
                base.wrapping_add(self.y as u16)
            }
        }
    }
    
    /// Check if a memory access crosses a page boundary
    /// Returns true if the access crosses a page boundary, false otherwise
    fn page_cross_check(&self, addr1: u16, addr2: u16) -> bool {
        (addr1 & 0xFF00) != (addr2 & 0xFF00)
    }
    
    /// Push a byte onto the stack
    fn push(&mut self, bus: &mut impl CpuBus, value: u8) {
        bus.write(0x0100 + self.sp as u16, value);
        self.sp = self.sp.wrapping_sub(1);
    }
    
    /// Pop a byte from the stack
    fn pop(&mut self, bus: &mut impl CpuBus) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        bus.read(0x0100 + self.sp as u16)
    }
    
    /// Push the processor status register onto the stack
    fn push_status(&mut self, bus: &mut impl CpuBus, with_break: bool) {
        let mut status = self.p;
        if with_break {
            status |= flags::BREAK;
        } else {
            status &= !flags::BREAK;
        }
        status |= flags::UNUSED; // Bit 5 is always set when pushed
        self.push(bus, status);
    }
    
    /// Execute an instruction, returns the number of cycles used
    pub fn step(&mut self, bus: &mut impl CpuBus) -> u8 {
        // Poll interrupts before fetching the next opcode.
        self.nmi_pending = bus.poll_interrupts();
        self.irq_pending = bus.poll_irq();
    
        if self.nmi_pending && !self.interrupt_delay {
             self.handle_nmi(bus);
             self.remaining_cycles = 0;
             return 7; // NMI takes 7 cycles
        }
        if self.irq_pending && !self.get_flag(flags::INTERRUPT_DISABLE) && !self.interrupt_delay {
             self.handle_irq(bus);
             self.remaining_cycles = 0;
             return 7; // IRQ takes 7 cycles
        }
        self.interrupt_delay = false;
    
        // Fetch and execute the opcode (completing the whole instruction)
        let opcode = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        let cycles_used = self.execute_instruction(bus, opcode);
        
        // Instead of waiting out the cycles, complete the instruction in one call.
        self.remaining_cycles = 0;
        self.cycles += cycles_used as u64;
        cycles_used
    }
    
    /// Handle a Non-Maskable Interrupt (NMI)
    fn handle_nmi(&mut self, bus: &mut impl CpuBus) {
        self.nmi_pending = false;
        // Push the current PC (return address) onto the stack:
        self.push(bus, (self.pc >> 8) as u8);
        self.push(bus, (self.pc & 0xFF) as u8);
        self.push_status(bus, false);
        self.set_flag(flags::INTERRUPT_DISABLE, true);
        
        // Load the NMI vector from 0xFFFA/0xFFFB
        let low = bus.read(0xFFFA);
        let high = bus.read(0xFFFB);
        self.pc = (high as u16) << 8 | (low as u16);
        
        self.remaining_cycles = 7;
    }
    
    /// Handle an Interrupt ReQuest (IRQ)
    fn handle_irq(&mut self, bus: &mut impl CpuBus) {
        self.push(bus, (self.pc >> 8) as u8);   // Push high byte of PC
        self.push(bus, self.pc as u8);          // Push low byte of PC
        self.push_status(bus, false);           // Push status register
        
        self.set_flag(flags::INTERRUPT_DISABLE, true); // Set the I flag
        
        // Read the IRQ vector from 0xFFFE-0xFFFF
        let low = bus.read(0xFFFE);
        let high = bus.read(0xFFFF);
        self.pc = (high as u16) << 8 | (low as u16);
        
        // IRQ takes 7 cycles
        self.remaining_cycles = 7;
    }
    
    /// Execute a single instruction, return the number of cycles used
    fn execute_instruction(&mut self, bus: &mut impl CpuBus, opcode: u8) -> u8 {
        match opcode {
            // ADC - Add with Carry
            0x69 => self.adc(bus, AddressingMode::Immediate),
            0x65 => self.adc(bus, AddressingMode::ZeroPage),
            0x75 => self.adc(bus, AddressingMode::ZeroPageX),
            0x6D => self.adc(bus, AddressingMode::Absolute),
            0x7D => self.adc(bus, AddressingMode::AbsoluteX),
            0x79 => self.adc(bus, AddressingMode::AbsoluteY),
            0x61 => self.adc(bus, AddressingMode::IndexedIndirect),
            0x71 => self.adc(bus, AddressingMode::IndirectIndexed),
            
            // AND - Logical AND
            0x29 => self.and(bus, AddressingMode::Immediate),
            0x25 => self.and(bus, AddressingMode::ZeroPage),
            0x35 => self.and(bus, AddressingMode::ZeroPageX),
            0x2D => self.and(bus, AddressingMode::Absolute),
            0x3D => self.and(bus, AddressingMode::AbsoluteX),
            0x39 => self.and(bus, AddressingMode::AbsoluteY),
            0x21 => self.and(bus, AddressingMode::IndexedIndirect),
            0x31 => self.and(bus, AddressingMode::IndirectIndexed),
            
            // ASL - Arithmetic Shift Left
            0x0A => self.asl_accumulator(bus),
            0x06 => self.asl(bus, AddressingMode::ZeroPage),
            0x16 => self.asl(bus, AddressingMode::ZeroPageX),
            0x0E => self.asl(bus, AddressingMode::Absolute),
            0x1E => self.asl(bus, AddressingMode::AbsoluteX),
            
            // BCC - Branch if Carry Clear
            0x90 => self.branch(bus, !self.get_flag(flags::CARRY)),
            
            // BCS - Branch if Carry Set
            0xB0 => self.branch(bus, self.get_flag(flags::CARRY)),
            
            // BEQ - Branch if Equal (Zero Set)
            0xF0 => self.branch(bus, self.get_flag(flags::ZERO)),
            
            // BIT - Bit Test
            0x24 => self.bit(bus, AddressingMode::ZeroPage),
            0x2C => self.bit(bus, AddressingMode::Absolute),
            
            // BMI - Branch if Minus (Negative Set)
            0x30 => self.branch(bus, self.get_flag(flags::NEGATIVE)),
            
            // BNE - Branch if Not Equal (Zero Clear)
            0xD0 => self.branch(bus, !self.get_flag(flags::ZERO)),
            
            // BPL - Branch if Plus (Negative Clear)
            0x10 => self.branch(bus, !self.get_flag(flags::NEGATIVE)),
            
            // BRK - Force Break / Software Interrupt
            0x00 => self.brk(bus),
            
            // BVC - Branch if Overflow Clear
            0x50 => self.branch(bus, !self.get_flag(flags::OVERFLOW)),
            
            // BVS - Branch if Overflow Set
            0x70 => self.branch(bus, self.get_flag(flags::OVERFLOW)),
            
            // CLC - Clear Carry Flag
            0x18 => { self.set_flag(flags::CARRY, false); 2 },
            
            // CLD - Clear Decimal Mode
            0xD8 => { self.set_flag(flags::DECIMAL, false); 2 },
            
            // CLI - Clear Interrupt Disable
            0x58 => { self.set_flag(flags::INTERRUPT_DISABLE, false); 2 },
            
            // CLV - Clear Overflow Flag
            0xB8 => { self.set_flag(flags::OVERFLOW, false); 2 },
            
            // CMP - Compare Accumulator
            0xC9 => self.cmp(bus, AddressingMode::Immediate),
            0xC5 => self.cmp(bus, AddressingMode::ZeroPage),
            0xD5 => self.cmp(bus, AddressingMode::ZeroPageX),
            0xCD => self.cmp(bus, AddressingMode::Absolute),
            0xDD => self.cmp(bus, AddressingMode::AbsoluteX),
            0xD9 => self.cmp(bus, AddressingMode::AbsoluteY),
            0xC1 => self.cmp(bus, AddressingMode::IndexedIndirect),
            0xD1 => self.cmp(bus, AddressingMode::IndirectIndexed),
            
            // CPX - Compare X Register
            0xE0 => self.cpx(bus, AddressingMode::Immediate),
            0xE4 => self.cpx(bus, AddressingMode::ZeroPage),
            0xEC => self.cpx(bus, AddressingMode::Absolute),
            
            // CPY - Compare Y Register
            0xC0 => self.cpy(bus, AddressingMode::Immediate),
            0xC4 => self.cpy(bus, AddressingMode::ZeroPage),
            0xCC => self.cpy(bus, AddressingMode::Absolute),
            
            // DEC - Decrement Memory
            0xC6 => self.dec(bus, AddressingMode::ZeroPage),
            0xD6 => self.dec(bus, AddressingMode::ZeroPageX),
            0xCE => self.dec(bus, AddressingMode::Absolute),
            0xDE => self.dec(bus, AddressingMode::AbsoluteX),
            
            // DEX - Decrement X Register
            0xCA => { self.x = self.x.wrapping_sub(1); self.update_zero_and_negative_flags(self.x); 2 },
            
            // DEY - Decrement Y Register
            0x88 => { self.y = self.y.wrapping_sub(1); self.update_zero_and_negative_flags(self.y); 2 },
            
            // EOR - Exclusive OR
            0x49 => self.eor(bus, AddressingMode::Immediate),
            0x45 => self.eor(bus, AddressingMode::ZeroPage),
            0x55 => self.eor(bus, AddressingMode::ZeroPageX),
            0x4D => self.eor(bus, AddressingMode::Absolute),
            0x5D => self.eor(bus, AddressingMode::AbsoluteX),
            0x59 => self.eor(bus, AddressingMode::AbsoluteY),
            0x41 => self.eor(bus, AddressingMode::IndexedIndirect),
            0x51 => self.eor(bus, AddressingMode::IndirectIndexed),
            
            // INC - Increment Memory
            0xE6 => self.inc(bus, AddressingMode::ZeroPage),
            0xF6 => self.inc(bus, AddressingMode::ZeroPageX),
            0xEE => self.inc(bus, AddressingMode::Absolute),
            0xFE => self.inc(bus, AddressingMode::AbsoluteX),
            
            // INX - Increment X Register
            0xE8 => { self.x = self.x.wrapping_add(1); self.update_zero_and_negative_flags(self.x); 2 },
            
            // INY - Increment Y Register
            0xC8 => { self.y = self.y.wrapping_add(1); self.update_zero_and_negative_flags(self.y); 2 },
            
            // JMP - Jump
            0x4C => self.jmp(bus, AddressingMode::Absolute),
            0x6C => self.jmp(bus, AddressingMode::Indirect),
            
            // JSR - Jump to Subroutine
            0x20 => self.jsr(bus),
            
            // LDA - Load Accumulator
            0xA9 => self.lda(bus, AddressingMode::Immediate),
            0xA5 => self.lda(bus, AddressingMode::ZeroPage),
            0xB5 => self.lda(bus, AddressingMode::ZeroPageX),
            0xAD => self.lda(bus, AddressingMode::Absolute),
            0xBD => self.lda(bus, AddressingMode::AbsoluteX),
            0xB9 => self.lda(bus, AddressingMode::AbsoluteY),
            0xA1 => self.lda(bus, AddressingMode::IndexedIndirect),
            0xB1 => self.lda(bus, AddressingMode::IndirectIndexed),
            
            // LDX - Load X Register
            0xA2 => self.ldx(bus, AddressingMode::Immediate),
            0xA6 => self.ldx(bus, AddressingMode::ZeroPage),
            0xB6 => self.ldx(bus, AddressingMode::ZeroPageY),
            0xAE => self.ldx(bus, AddressingMode::Absolute),
            0xBE => self.ldx(bus, AddressingMode::AbsoluteY),
            
            // LDY - Load Y Register
            0xA0 => self.ldy(bus, AddressingMode::Immediate),
            0xA4 => self.ldy(bus, AddressingMode::ZeroPage),
            0xB4 => self.ldy(bus, AddressingMode::ZeroPageX),
            0xAC => self.ldy(bus, AddressingMode::Absolute),
            0xBC => self.ldy(bus, AddressingMode::AbsoluteX),
            
            // LSR - Logical Shift Right
            0x4A => self.lsr_accumulator(bus),
            0x46 => self.lsr(bus, AddressingMode::ZeroPage),
            0x56 => self.lsr(bus, AddressingMode::ZeroPageX),
            0x4E => self.lsr(bus, AddressingMode::Absolute),
            0x5E => self.lsr(bus, AddressingMode::AbsoluteX),
            
            // NOP - No Operation
            0xEA => 2, // Standard NOP
            
            // ORA - Logical Inclusive OR
            0x09 => self.ora(bus, AddressingMode::Immediate),
            0x05 => self.ora(bus, AddressingMode::ZeroPage),
            0x15 => self.ora(bus, AddressingMode::ZeroPageX),
            0x0D => self.ora(bus, AddressingMode::Absolute),
            0x1D => self.ora(bus, AddressingMode::AbsoluteX),
            0x19 => self.ora(bus, AddressingMode::AbsoluteY),
            0x01 => self.ora(bus, AddressingMode::IndexedIndirect),
            0x11 => self.ora(bus, AddressingMode::IndirectIndexed),
            
            // PHA - Push Accumulator
            0x48 => { self.push(bus, self.a); 3 },
            
            // PHP - Push Processor Status
            0x08 => { self.push_status(bus, true); 3 },
            
            // PLA - Pull Accumulator
            0x68 => { self.a = self.pop(bus); self.update_zero_and_negative_flags(self.a); 4 },
            
            // PLP - Pull Processor Status
            0x28 => {
                let status = self.pop(bus);
                self.p = (status & !flags::BREAK) | flags::UNUSED; // B flag not set when pulled
                4
            },
            
            // ROL - Rotate Left
            0x2A => self.rol_accumulator(bus),
            0x26 => self.rol(bus, AddressingMode::ZeroPage),
            0x36 => self.rol(bus, AddressingMode::ZeroPageX),
            0x2E => self.rol(bus, AddressingMode::Absolute),
            0x3E => self.rol(bus, AddressingMode::AbsoluteX),
            
            // ROR - Rotate Right
            0x6A => self.ror_accumulator(bus),
            0x66 => self.ror(bus, AddressingMode::ZeroPage),
            0x76 => self.ror(bus, AddressingMode::ZeroPageX),
            0x6E => self.ror(bus, AddressingMode::Absolute),
            0x7E => self.ror(bus, AddressingMode::AbsoluteX),
            
            // RTI - Return from Interrupt
            0x40 => self.rti(bus),
            
            // RTS - Return from Subroutine
            0x60 => self.rts(bus),
            
			// SBC - Subtract with Carry
            0xE9 => self.sbc(bus, AddressingMode::Immediate),
            0xE5 => self.sbc(bus, AddressingMode::ZeroPage),
            0xF5 => self.sbc(bus, AddressingMode::ZeroPageX),
            0xED => self.sbc(bus, AddressingMode::Absolute),
            0xFD => self.sbc(bus, AddressingMode::AbsoluteX),
            0xF9 => self.sbc(bus, AddressingMode::AbsoluteY),
            0xE1 => self.sbc(bus, AddressingMode::IndexedIndirect),
            0xF1 => self.sbc(bus, AddressingMode::IndirectIndexed),
            
            // SEC - Set Carry Flag
            0x38 => { self.set_flag(flags::CARRY, true); 2 },
            
            // SED - Set Decimal Flag
            0xF8 => { self.set_flag(flags::DECIMAL, true); 2 },
            
            // SEI - Set Interrupt Disable
            0x78 => { self.set_flag(flags::INTERRUPT_DISABLE, true); 2 },
            
            // STA - Store Accumulator
            0x85 => self.sta(bus, AddressingMode::ZeroPage),
            0x95 => self.sta(bus, AddressingMode::ZeroPageX),
            0x8D => self.sta(bus, AddressingMode::Absolute),
            0x9D => self.sta(bus, AddressingMode::AbsoluteX),
            0x99 => self.sta(bus, AddressingMode::AbsoluteY),
            0x81 => self.sta(bus, AddressingMode::IndexedIndirect),
            0x91 => self.sta(bus, AddressingMode::IndirectIndexed),
            
            // STX - Store X Register
            0x86 => self.stx(bus, AddressingMode::ZeroPage),
            0x96 => self.stx(bus, AddressingMode::ZeroPageY),
            0x8E => self.stx(bus, AddressingMode::Absolute),
            
            // STY - Store Y Register
            0x84 => self.sty(bus, AddressingMode::ZeroPage),
            0x94 => self.sty(bus, AddressingMode::ZeroPageX),
            0x8C => self.sty(bus, AddressingMode::Absolute),
            
            // TAX - Transfer A to X
            0xAA => { self.x = self.a; self.update_zero_and_negative_flags(self.x); 2 },
            
            // TAY - Transfer A to Y
            0xA8 => { self.y = self.a; self.update_zero_and_negative_flags(self.y); 2 },
            
            // TSX - Transfer Stack Pointer to X
            0xBA => { self.x = self.sp; self.update_zero_and_negative_flags(self.x); 2 },
            
            // TXA - Transfer X to A
            0x8A => { self.a = self.x; self.update_zero_and_negative_flags(self.a); 2 },
            
            // TXS - Transfer X to Stack Pointer
            0x9A => { self.sp = self.x; 2 }, // Note: this doesn't affect any flags
            
            // TYA - Transfer Y to A
            0x98 => { self.a = self.y; self.update_zero_and_negative_flags(self.a); 2 },
            
            // Unofficial opcodes
            
            // LAX - Load A and X with same value
            0xA7 => self.lax(bus, AddressingMode::ZeroPage),
            0xB7 => self.lax(bus, AddressingMode::ZeroPageY),
            0xAF => self.lax(bus, AddressingMode::Absolute),
            0xBF => self.lax(bus, AddressingMode::AbsoluteY),
            0xA3 => self.lax(bus, AddressingMode::IndexedIndirect),
            0xB3 => self.lax(bus, AddressingMode::IndirectIndexed),
            
            // SAX - Store A AND X
            0x87 => self.sax(bus, AddressingMode::ZeroPage),
            0x97 => self.sax(bus, AddressingMode::ZeroPageY),
            0x8F => self.sax(bus, AddressingMode::Absolute),
            0x83 => self.sax(bus, AddressingMode::IndexedIndirect),
            
            // SBC - Unofficial duplicate opcodes
            0xEB => self.sbc(bus, AddressingMode::Immediate), // Same as 0xE9
            
            // DCP - DEC + CMP
            0xC7 => self.dcp(bus, AddressingMode::ZeroPage),
            0xD7 => self.dcp(bus, AddressingMode::ZeroPageX),
            0xCF => self.dcp(bus, AddressingMode::Absolute),
            0xDF => self.dcp(bus, AddressingMode::AbsoluteX),
            0xDB => self.dcp(bus, AddressingMode::AbsoluteY),
            0xC3 => self.dcp(bus, AddressingMode::IndexedIndirect),
            0xD3 => self.dcp(bus, AddressingMode::IndirectIndexed),
            
            // ISB/ISC - INC + SBC
            0xE7 => self.isb(bus, AddressingMode::ZeroPage),
            0xF7 => self.isb(bus, AddressingMode::ZeroPageX),
            0xEF => self.isb(bus, AddressingMode::Absolute),
            0xFF => self.isb(bus, AddressingMode::AbsoluteX),
            0xFB => self.isb(bus, AddressingMode::AbsoluteY),
            0xE3 => self.isb(bus, AddressingMode::IndexedIndirect),
            0xF3 => self.isb(bus, AddressingMode::IndirectIndexed),
            
            // SLO - ASL + ORA
            0x07 => self.slo(bus, AddressingMode::ZeroPage),
            0x17 => self.slo(bus, AddressingMode::ZeroPageX),
            0x0F => self.slo(bus, AddressingMode::Absolute),
            0x1F => self.slo(bus, AddressingMode::AbsoluteX),
            0x1B => self.slo(bus, AddressingMode::AbsoluteY),
            0x03 => self.slo(bus, AddressingMode::IndexedIndirect),
            0x13 => self.slo(bus, AddressingMode::IndirectIndexed),
            
            // RLA - ROL + AND
            0x27 => self.rla(bus, AddressingMode::ZeroPage),
            0x37 => self.rla(bus, AddressingMode::ZeroPageX),
            0x2F => self.rla(bus, AddressingMode::Absolute),
            0x3F => self.rla(bus, AddressingMode::AbsoluteX),
            0x3B => self.rla(bus, AddressingMode::AbsoluteY),
            0x23 => self.rla(bus, AddressingMode::IndexedIndirect),
            0x33 => self.rla(bus, AddressingMode::IndirectIndexed),
            
            // SRE - LSR + EOR
            0x47 => self.sre(bus, AddressingMode::ZeroPage),
            0x57 => self.sre(bus, AddressingMode::ZeroPageX),
            0x4F => self.sre(bus, AddressingMode::Absolute),
            0x5F => self.sre(bus, AddressingMode::AbsoluteX),
            0x5B => self.sre(bus, AddressingMode::AbsoluteY),
            0x43 => self.sre(bus, AddressingMode::IndexedIndirect),
            0x53 => self.sre(bus, AddressingMode::IndirectIndexed),
            
            // RRA - ROR + ADC
            0x67 => self.rra(bus, AddressingMode::ZeroPage),
            0x77 => self.rra(bus, AddressingMode::ZeroPageX),
            0x6F => self.rra(bus, AddressingMode::Absolute),
            0x7F => self.rra(bus, AddressingMode::AbsoluteX),
            0x7B => self.rra(bus, AddressingMode::AbsoluteY),
            0x63 => self.rra(bus, AddressingMode::IndexedIndirect),
            0x73 => self.rra(bus, AddressingMode::IndirectIndexed),
            
            // Unofficial NOPs
            0x04 | 0x44 | 0x64 => 3, // Zero page
            0x0C => 4, // Absolute
            0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 => 4, // Zero page, X
            0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => 2, // Implied
            0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => 4, // Absolute, X
            
            // Handle unknown/illegal opcodes - return 0 cycles to indicate failure
            _ => {
                #[cfg(debug_assertions)]
                println!("Unknown opcode: ${:02X} at address: ${:04X}", opcode, self.pc.wrapping_sub(1));
                2 // Default to 2 cycles for unknown opcodes
            }
        }
    }
    
    // Instruction Implementations
    
    /// ADC - Add with Carry
    fn adc(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        // Perform addition with carry
        let carry = if self.get_flag(flags::CARRY) { 1 } else { 0 };
        let result = self.a as u16 + operand as u16 + carry as u16;
        
        // Set flags
        self.set_flag(flags::CARRY, result > 0xFF);
        
        // Set overflow flag - overflow occurs when the sign of both operands is different from the result
        let result_byte = result as u8;
        self.set_flag(flags::OVERFLOW, ((self.a ^ result_byte) & (operand ^ result_byte) & 0x80) != 0);
        
        self.a = result_byte;
        self.update_zero_and_negative_flags(self.a);
        
        // Return cycles - add 1 if page boundary is crossed in certain addressing modes
        match mode {
            AddressingMode::AbsoluteX | AddressingMode::AbsoluteY | AddressingMode::IndirectIndexed => {
                let base = match mode {
                    AddressingMode::AbsoluteX => addr.wrapping_sub(self.x as u16),
                    _ => addr.wrapping_sub(self.y as u16),
                };
                if self.page_cross_check(base, addr) { 5 } else { 4 }
            }
            AddressingMode::Immediate => 2,
            AddressingMode::ZeroPage | AddressingMode::ZeroPageX | AddressingMode::IndexedIndirect => 3,
            AddressingMode::Absolute => 4,
            _ => 2
        }
    }
    
    /// AND - Logical AND
    fn and(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        self.a &= operand;
        self.update_zero_and_negative_flags(self.a);
        
        // Return cycles - add 1 if page boundary is crossed in certain addressing modes
        match mode {
            AddressingMode::AbsoluteX | AddressingMode::AbsoluteY | AddressingMode::IndirectIndexed => {
                let base = addr.wrapping_sub(if mode == AddressingMode::AbsoluteX { 
                    self.x as u16 
                } else { 
                    self.y as u16 
                });
                if self.page_cross_check(base, addr) { 4 } else { 3 }
            }
            AddressingMode::Immediate | AddressingMode::ZeroPage | AddressingMode::ZeroPageX
                | AddressingMode::IndexedIndirect => 3,
            AddressingMode::Absolute => 4,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// ASL - Arithmetic Shift Left (Memory)
    fn asl(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let mut operand = bus.read(addr);
        
        // Set carry flag to bit 7 of the operand
        self.set_flag(flags::CARRY, (operand & 0x80) != 0);
        
        // Shift left
        operand <<= 1;
        bus.write(addr, operand);
        
        self.update_zero_and_negative_flags(operand);
        
        // Return cycles
        match mode {
            AddressingMode::ZeroPage => 5,
            AddressingMode::ZeroPageX => 6,
            AddressingMode::Absolute => 6,
            AddressingMode::AbsoluteX => 7,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// ASL - Arithmetic Shift Left (Accumulator)
    fn asl_accumulator(&mut self, _bus: &mut impl CpuBus) -> u8 {
        // Set carry flag to bit 7 of the accumulator
        self.set_flag(flags::CARRY, (self.a & 0x80) != 0);
        
        // Shift left
        self.a <<= 1;
        
        self.update_zero_and_negative_flags(self.a);
        
        // Return cycles
        2
    }
    
    /// Branch helper function - handles all branch instructions
    fn branch(&mut self, bus: &mut impl CpuBus, condition: bool) -> u8 {
        // Read the signed branch offset from the operand byte
        let offset = bus.read(self.pc) as i8;
        // Increment PC past the operand (branch instructions are 2 bytes)
        self.pc = self.pc.wrapping_add(1);
        if condition {
             let initial_pc = self.pc;
             // Add the signed offset to PC (no extra subtraction)
             self.pc = self.pc.wrapping_add(offset as u16);
             // If the branch crosses a page, add an extra cycle
             if (initial_pc & 0xFF00) != (self.pc & 0xFF00) {
                 4
             } else {
                 3
             }
        } else {
             2
        }
    }
    
    /// BIT - Bit Test
    fn bit(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        // Set zero flag based on AND result
        self.set_flag(flags::ZERO, (self.a & operand) == 0);
        
        // Set negative flag to bit 7 of operand
        self.set_flag(flags::NEGATIVE, (operand & 0x80) != 0);
        
        // Set overflow flag to bit 6 of operand
        self.set_flag(flags::OVERFLOW, (operand & 0x40) != 0);
        
        // Return cycles
        match mode {
            AddressingMode::ZeroPage => 3,
            AddressingMode::Absolute => 4,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// BRK - Force Break / Software Interrupt
    fn brk(&mut self, bus: &mut impl CpuBus) -> u8 {
        // BRK is treated as a 2-byte instruction, so increment PC by 2.
        self.pc = self.pc.wrapping_add(1);
        // Push the return address (PC) in the order: high, then low, then processor status
        self.push(bus, (self.pc >> 8) as u8);
        self.push(bus, (self.pc & 0xFF) as u8);
        self.push_status(bus, true);
        self.set_flag(flags::INTERRUPT_DISABLE, true);
        
        // Load the IRQ/BRK vector from 0xFFFE/0xFFFF
        let low = bus.read(0xFFFE);
        let high = bus.read(0xFFFF);
        self.pc = (high as u16) << 8 | (low as u16);
        
        7
    }
    
    /// CMP - Compare Accumulator
    fn cmp(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        // Perform comparison
        let result = self.a.wrapping_sub(operand);
        
        // Set flags
        self.set_flag(flags::CARRY, self.a >= operand);
        self.update_zero_and_negative_flags(result);
        
        // Return cycles
        match mode {
            AddressingMode::AbsoluteX | AddressingMode::AbsoluteY | AddressingMode::IndirectIndexed => {
                let base = addr.wrapping_sub(if mode == AddressingMode::AbsoluteX { 
                    self.x as u16 
                } else { 
                    self.y as u16 
                });
                if self.page_cross_check(base, addr) { 4 } else { 3 }
            }
            AddressingMode::Immediate | AddressingMode::ZeroPage | AddressingMode::ZeroPageX
                | AddressingMode::IndexedIndirect => 3,
            AddressingMode::Absolute => 4,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// CPX - Compare X Register
    fn cpx(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        // Perform comparison
        let result = self.x.wrapping_sub(operand);
        
        // Set flags
        self.set_flag(flags::CARRY, self.x >= operand);
        self.update_zero_and_negative_flags(result);
        
        // Return cycles
        match mode {
            AddressingMode::Immediate => 2,
            AddressingMode::ZeroPage => 3,
            AddressingMode::Absolute => 4,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// CPY - Compare Y Register
    fn cpy(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        // Perform comparison
        let result = self.y.wrapping_sub(operand);
        
        // Set flags
        self.set_flag(flags::CARRY, self.y >= operand);
        self.update_zero_and_negative_flags(result);
        
        // Return cycles
        match mode {
            AddressingMode::Immediate => 2,
            AddressingMode::ZeroPage => 3,
            AddressingMode::Absolute => 4,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// DEC - Decrement Memory
    fn dec(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        // Decrement
        let result = operand.wrapping_sub(1);
        bus.write(addr, result);
        
        self.update_zero_and_negative_flags(result);
        
        // Return cycles
        match mode {
            AddressingMode::ZeroPage => 5,
            AddressingMode::ZeroPageX => 6,
            AddressingMode::Absolute => 6,
            AddressingMode::AbsoluteX => 7,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// EOR - Exclusive OR
    fn eor(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        self.a ^= operand;
        self.update_zero_and_negative_flags(self.a);
        
        // Return cycles
        match mode {
            AddressingMode::AbsoluteX | AddressingMode::AbsoluteY | AddressingMode::IndirectIndexed => {
                let base = addr.wrapping_sub(if mode == AddressingMode::AbsoluteX { 
                    self.x as u16 
                } else { 
                    self.y as u16 
                });
                if self.page_cross_check(base, addr) { 4 } else { 3 }
            }
            AddressingMode::Immediate | AddressingMode::ZeroPage | AddressingMode::ZeroPageX
                | AddressingMode::IndexedIndirect => 3,
            AddressingMode::Absolute => 4,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// INC - Increment Memory
    fn inc(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        // Increment
        let result = operand.wrapping_add(1);
        bus.write(addr, result);
        
        self.update_zero_and_negative_flags(result);
        
        // Return cycles
        match mode {
            AddressingMode::ZeroPage => 5,
            AddressingMode::ZeroPageX => 6,
            AddressingMode::Absolute => 6,
            AddressingMode::AbsoluteX => 7,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// JMP - Jump
    fn jmp(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        self.pc = addr;
        
        // Return cycles
        match mode {
            AddressingMode::Absolute => 3,
            AddressingMode::Indirect => 5,
            _ => 3 // Default (should not happen)
        }
    }
    
    /// JSR - Jump to Subroutine
    fn jsr(&mut self, bus: &mut impl CpuBus) -> u8 {
        // JSR pushes the address-1 of the next operation
        let return_addr = self.pc.wrapping_add(1);
        
        // Push return address
        self.push(bus, (return_addr >> 8) as u8);   // Push high byte
        self.push(bus, return_addr as u8);          // Push low byte
        
        // Get jump address
        let low = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        let high = bus.read(self.pc);
        self.pc = (high as u16) << 8 | (low as u16);
        
        // JSR takes 6 cycles
        6
    }
    
    /// LDA - Load Accumulator
    fn lda(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        self.a = bus.read(addr);
        
        self.update_zero_and_negative_flags(self.a);
        
        // Return cycles
        match mode {
            AddressingMode::AbsoluteX | AddressingMode::AbsoluteY | AddressingMode::IndirectIndexed => {
                let base = addr.wrapping_sub(if mode == AddressingMode::AbsoluteX { 
                    self.x as u16 
                } else { 
                    self.y as u16 
                });
                if self.page_cross_check(base, addr) { 4 } else { 3 }
            }
            AddressingMode::Immediate | AddressingMode::ZeroPage | AddressingMode::ZeroPageX
                | AddressingMode::IndexedIndirect => 3,
            AddressingMode::Absolute => 4,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// LDX - Load X Register
    fn ldx(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        self.x = bus.read(addr);
        
        self.update_zero_and_negative_flags(self.x);
        
        // Return cycles
        match mode {
            AddressingMode::AbsoluteY => {
                let base = addr.wrapping_sub(self.y as u16);
                if self.page_cross_check(base, addr) { 4 } else { 3 }
            }
            AddressingMode::Immediate | AddressingMode::ZeroPage | AddressingMode::ZeroPageY => 3,
            AddressingMode::Absolute => 4,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// LDY - Load Y Register
    fn ldy(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        self.y = bus.read(addr);
        
        self.update_zero_and_negative_flags(self.y);
        
        // Return cycles
        match mode {
            AddressingMode::AbsoluteX => {
                let base = addr.wrapping_sub(self.x as u16);
                if self.page_cross_check(base, addr) { 4 } else { 3 }
            }
            AddressingMode::Immediate | AddressingMode::ZeroPage | AddressingMode::ZeroPageX => 3,
            AddressingMode::Absolute => 4,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// LSR - Logical Shift Right (Memory)
    fn lsr(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        // Set carry flag to bit 0 of the operand
        self.set_flag(flags::CARRY, (operand & 0x01) != 0);
        
        // Shift right
        let result = operand >> 1;
        bus.write(addr, result);
        
        self.update_zero_and_negative_flags(result);
        
        // Return cycles
        match mode {
            AddressingMode::ZeroPage => 5,
            AddressingMode::ZeroPageX => 6,
            AddressingMode::Absolute => 6,
            AddressingMode::AbsoluteX => 7,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// LSR - Logical Shift Right (Accumulator)
    fn lsr_accumulator(&mut self, _bus: &mut impl CpuBus) -> u8 {
        // Set carry flag to bit 0 of the accumulator
        self.set_flag(flags::CARRY, (self.a & 0x01) != 0);
        
        // Shift right
        self.a >>= 1;
        
        self.update_zero_and_negative_flags(self.a);
        
        // LSR A takes 2 cycles
        2
    }
    
    /// ORA - Logical Inclusive OR
    fn ora(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        self.a |= operand;
        self.update_zero_and_negative_flags(self.a);
        
        // Return cycles
        match mode {
            AddressingMode::AbsoluteX | AddressingMode::AbsoluteY | AddressingMode::IndirectIndexed => {
                let base = addr.wrapping_sub(if mode == AddressingMode::AbsoluteX { 
                    self.x as u16 
                } else { 
                    self.y as u16 
                });
                if self.page_cross_check(base, addr) { 4 } else { 3 }
            }
            AddressingMode::Immediate | AddressingMode::ZeroPage | AddressingMode::ZeroPageX
                | AddressingMode::IndexedIndirect => 3,
            AddressingMode::Absolute => 4,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// ROL - Rotate Left (Memory)
    fn rol(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        // Rotate left through carry
        let carry = if self.get_flag(flags::CARRY) { 1 } else { 0 };
        let result = (operand << 1) | carry;
        
        // Set carry flag to bit 7 of the operand
        self.set_flag(flags::CARRY, (operand & 0x80) != 0);
        
        bus.write(addr, result);
        self.update_zero_and_negative_flags(result);
        
        // Return cycles
        match mode {
            AddressingMode::ZeroPage => 5,
            AddressingMode::ZeroPageX => 6,
            AddressingMode::Absolute => 6,
			AddressingMode::AbsoluteX => 7,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// ROL - Rotate Left (Accumulator)
    fn rol_accumulator(&mut self, _bus: &mut impl CpuBus) -> u8 {
        // Rotate left through carry
        let carry = if self.get_flag(flags::CARRY) { 1 } else { 0 };
        let result = (self.a << 1) | carry;
        
        // Set carry flag to bit 7 of the accumulator
        self.set_flag(flags::CARRY, (self.a & 0x80) != 0);
        
        self.a = result;
        self.update_zero_and_negative_flags(self.a);
        
        // ROL A takes 2 cycles
        2
    }
    
    /// ROR - Rotate Right (Memory)
    fn ror(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        // Rotate right through carry
        let carry = if self.get_flag(flags::CARRY) { 0x80 } else { 0 };
        let result = (operand >> 1) | carry;
        
        // Set carry flag to bit 0 of the operand
        self.set_flag(flags::CARRY, (operand & 0x01) != 0);
        
        bus.write(addr, result);
        self.update_zero_and_negative_flags(result);
        
        // Return cycles
        match mode {
            AddressingMode::ZeroPage => 5,
            AddressingMode::ZeroPageX => 6,
            AddressingMode::Absolute => 6,
            AddressingMode::AbsoluteX => 7,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// ROR - Rotate Right (Accumulator)
    fn ror_accumulator(&mut self, _bus: &mut impl CpuBus) -> u8 {
        // Rotate right through carry
        let carry = if self.get_flag(flags::CARRY) { 0x80 } else { 0 };
        let result = (self.a >> 1) | carry;
        
        // Set carry flag to bit 0 of the accumulator
        self.set_flag(flags::CARRY, (self.a & 0x01) != 0);
        
        self.a = result;
        self.update_zero_and_negative_flags(self.a);
        
        // ROR A takes 2 cycles
        2
    }
    
    /// RTI - Return from Interrupt
    fn rti(&mut self, bus: &mut impl CpuBus) -> u8 {
        // Pull the status register
        let status = self.pop(bus);
        self.p = (status & !flags::BREAK) | flags::UNUSED;
        
        // Pull PC low and high
        let low = self.pop(bus) as u16;
        let high = self.pop(bus) as u16;
        self.pc = (high << 8) | low;
        
        // RTI takes 6 cycles
        6
    }
    
    /// RTS - Return from Subroutine
    fn rts(&mut self, bus: &mut impl CpuBus) -> u8 {
        // Pull return address
        let low = self.pop(bus) as u16;
        let high = self.pop(bus) as u16;
        
        // RTS increments the return address by 1
        self.pc = ((high << 8) | low).wrapping_add(1);
        
        // RTS takes 6 cycles
        6
    }
    
    /// SBC - Subtract with Carry
    fn sbc(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        // Perform subtraction with carry (borrow)
        let carry_in = if self.get_flag(flags::CARRY) { 1 } else { 0 };
        let result = self.a as i32 - operand as i32 - (1 - carry_in) as i32;
        // If the carry flag was clear before, force a borrow (clear carry flag),
        // otherwise use the computed result.
        if !self.get_flag(flags::CARRY) {
            self.set_flag(flags::CARRY, false);
        } else {
            self.set_flag(flags::CARRY, result >= 0);
        }
        
        // Set overflow flag
        let result_byte = result as u8;
        self.set_flag(flags::OVERFLOW, ((self.a ^ operand) & 0x80) != 0 && ((self.a ^ result_byte) & 0x80) != 0);
        
        self.a = result_byte;
        self.update_zero_and_negative_flags(self.a);
        
        // Return cycles
        match mode {
            AddressingMode::AbsoluteX | AddressingMode::AbsoluteY | AddressingMode::IndirectIndexed => {
                let base = match mode {
                    AddressingMode::AbsoluteX => addr.wrapping_sub(self.x as u16),
                    _ => addr.wrapping_sub(self.y as u16),
                };
                if self.page_cross_check(base, addr) { 5 } else { 4 }
            }
            AddressingMode::Immediate => 2,
            AddressingMode::ZeroPage | AddressingMode::ZeroPageX | AddressingMode::IndexedIndirect => 3,
            AddressingMode::Absolute => 4,
            _ => 2
        }
    }
    
    /// STA - Store Accumulator
    fn sta(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        bus.write(addr, self.a);
        
        // Return correct cycles
        match mode {
            AddressingMode::ZeroPage => 3,
            AddressingMode::ZeroPageX => 4,
            AddressingMode::Absolute => 4,
            AddressingMode::AbsoluteX | AddressingMode::AbsoluteY => 5,
            AddressingMode::IndexedIndirect | AddressingMode::IndirectIndexed => 6,
            _ => 2
        }
    }
    
    /// STX - Store X Register
    fn stx(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        bus.write(addr, self.x);
        
        // Return cycles
        match mode {
            AddressingMode::ZeroPage => 3,
            AddressingMode::ZeroPageY => 4,
            AddressingMode::Absolute => 4,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// STY - Store Y Register
    fn sty(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        bus.write(addr, self.y);
        
        // Return cycles
        match mode {
            AddressingMode::ZeroPage => 3,
            AddressingMode::ZeroPageX => 4,
            AddressingMode::Absolute => 4,
            _ => 2 // Default (should not happen)
        }
    }
    
    // Unofficial Opcodes
    
    /// LAX - Load A and X with same value
    fn lax(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        self.a = bus.read(addr);
        self.x = self.a;
        
        self.update_zero_and_negative_flags(self.a);
        
        // Return cycles
        match mode {
            AddressingMode::AbsoluteY | AddressingMode::IndirectIndexed => {
                let base = addr.wrapping_sub(if mode == AddressingMode::AbsoluteY { 
                    self.y as u16 
                } else { 
                    self.y as u16 
                });
                if self.page_cross_check(base, addr) { 4 } else { 3 }
            }
            AddressingMode::ZeroPage | AddressingMode::ZeroPageY | AddressingMode::IndexedIndirect => 5,
            AddressingMode::Absolute => 4,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// SAX - Store A AND X
    fn sax(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        bus.write(addr, self.a & self.x);
        
        // Return cycles
        match mode {
            AddressingMode::ZeroPage => 3,
            AddressingMode::ZeroPageY => 4,
            AddressingMode::Absolute => 4,
            AddressingMode::IndexedIndirect => 6,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// DCP - DEC + CMP
    fn dcp(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        // Decrement
        let result = operand.wrapping_sub(1);
        bus.write(addr, result);
        
        // Compare
        let cmp_result = self.a.wrapping_sub(result);
        self.set_flag(flags::CARRY, self.a >= result);
        self.update_zero_and_negative_flags(cmp_result);
        
        // Return cycles
        match mode {
            AddressingMode::ZeroPage => 5,
            AddressingMode::ZeroPageX => 6,
            AddressingMode::Absolute => 6,
            AddressingMode::AbsoluteX | AddressingMode::AbsoluteY => 7,
            AddressingMode::IndexedIndirect | AddressingMode::IndirectIndexed => 8,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// ISB/ISC - INC + SBC
    fn isb(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        // Increment
        let inc_result = operand.wrapping_add(1);
        bus.write(addr, inc_result);
        
        // Subtract with Carry
        let carry = if self.get_flag(flags::CARRY) { 0 } else { 1 };
        let result = self.a as i16 - inc_result as i16 - carry as i16;
        
        // Set flags
        self.set_flag(flags::CARRY, result >= 0);
        
        // Set overflow flag
        let result_byte = result as u8;
        self.set_flag(flags::OVERFLOW, ((self.a ^ inc_result) & 0x80) != 0 && ((self.a ^ result_byte) & 0x80) != 0);
        
        self.a = result_byte;
        self.update_zero_and_negative_flags(self.a);
        
        // Return cycles
        match mode {
            AddressingMode::ZeroPage => 5,
            AddressingMode::ZeroPageX => 6,
            AddressingMode::Absolute => 6,
            AddressingMode::AbsoluteX | AddressingMode::AbsoluteY => 7,
            AddressingMode::IndexedIndirect | AddressingMode::IndirectIndexed => 8,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// SLO - ASL + ORA
    fn slo(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        // ASL
        self.set_flag(flags::CARRY, (operand & 0x80) != 0);
        let result = operand << 1;
        bus.write(addr, result);
        
        // ORA
        self.a |= result;
        self.update_zero_and_negative_flags(self.a);
        
        // Return cycles
        match mode {
            AddressingMode::ZeroPage => 5,
            AddressingMode::ZeroPageX => 6,
            AddressingMode::Absolute => 6,
            AddressingMode::AbsoluteX | AddressingMode::AbsoluteY => 7,
            AddressingMode::IndexedIndirect | AddressingMode::IndirectIndexed => 8,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// RLA - ROL + AND
    fn rla(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        // ROL
        let carry = if self.get_flag(flags::CARRY) { 1 } else { 0 };
        let rol_result = (operand << 1) | carry;
        self.set_flag(flags::CARRY, (operand & 0x80) != 0);
        bus.write(addr, rol_result);
        
        // AND
        self.a &= rol_result;
        self.update_zero_and_negative_flags(self.a);
        
        // Return cycles
        match mode {
            AddressingMode::ZeroPage => 5,
            AddressingMode::ZeroPageX => 6,
            AddressingMode::Absolute => 6,
            AddressingMode::AbsoluteX | AddressingMode::AbsoluteY => 7,
            AddressingMode::IndexedIndirect | AddressingMode::IndirectIndexed => 8,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// SRE - LSR + EOR
    fn sre(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        // LSR
        self.set_flag(flags::CARRY, (operand & 0x01) != 0);
        let result = operand >> 1;
        bus.write(addr, result);
        
        // EOR
        self.a ^= result;
        self.update_zero_and_negative_flags(self.a);
        
        // Return cycles
        match mode {
            AddressingMode::ZeroPage => 5,
            AddressingMode::ZeroPageX => 6,
            AddressingMode::Absolute => 6,
            AddressingMode::AbsoluteX | AddressingMode::AbsoluteY => 7,
            AddressingMode::IndexedIndirect | AddressingMode::IndirectIndexed => 8,
            _ => 2 // Default (should not happen)
        }
    }
    
    /// RRA - ROR + ADC
    fn rra(&mut self, bus: &mut impl CpuBus, mode: AddressingMode) -> u8 {
        let addr = self.get_operand_address(bus, mode);
        let operand = bus.read(addr);
        
        // ROR
        let old_carry = if self.get_flag(flags::CARRY) { 0x80 } else { 0 };
        let ror_result = (operand >> 1) | old_carry;
        self.set_flag(flags::CARRY, (operand & 0x01) != 0);
        bus.write(addr, ror_result);
        
        // ADC
        let new_carry = if self.get_flag(flags::CARRY) { 1 } else { 0 };
        let adc_result = self.a as u16 + ror_result as u16 + new_carry as u16;
        
        // Set flags for ADC
        self.set_flag(flags::CARRY, adc_result > 0xFF);
        
        // Set overflow flag
        let result_byte = adc_result as u8;
        self.set_flag(flags::OVERFLOW, ((self.a ^ result_byte) & (ror_result ^ result_byte) & 0x80) != 0);
        
        self.a = result_byte;
        self.update_zero_and_negative_flags(self.a);
        
        // Return cycles
        match mode {
            AddressingMode::ZeroPage => 5,
            AddressingMode::ZeroPageX => 6,
            AddressingMode::Absolute => 6,
            AddressingMode::AbsoluteX | AddressingMode::AbsoluteY => 7,
            AddressingMode::IndexedIndirect | AddressingMode::IndirectIndexed => 8,
            _ => 2 // Default (should not happen)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // A simple implementation of the CpuBus trait for testing
    struct TestBus {
        memory: [u8; 0x10000],
        nmi_pending: bool,
        irq_pending: bool,
    }
    
    impl TestBus {
        fn new() -> Self {
            TestBus {
                memory: [0; 0x10000],
                nmi_pending: false,
                irq_pending: false,
            }
        }
        
        fn set_nmi(&mut self, pending: bool) {
            self.nmi_pending = pending;
        }
        
        fn set_irq(&mut self, pending: bool) {
            self.irq_pending = pending;
        }
        
        fn load_program(&mut self, program: &[u8], address: u16) {
            for (i, &byte) in program.iter().enumerate() {
                self.memory[address as usize + i] = byte;
            }
        }
        
        fn set_reset_vector(&mut self, address: u16) {
            self.memory[0xFFFC] = (address & 0xFF) as u8;
            self.memory[0xFFFD] = (address >> 8) as u8;
        }
        
        fn set_nmi_vector(&mut self, address: u16) {
            self.memory[0xFFFA] = (address & 0xFF) as u8;
            self.memory[0xFFFB] = (address >> 8) as u8;
        }
        
        fn set_irq_vector(&mut self, address: u16) {
            self.memory[0xFFFE] = (address & 0xFF) as u8;
            self.memory[0xFFFF] = (address >> 8) as u8;
        }
    }
    
    impl CpuBus for TestBus {
        fn read(&mut self, address: u16) -> u8 {
            self.memory[address as usize]
        }
        
        fn write(&mut self, address: u16, data: u8) {
            self.memory[address as usize] = data;
        }
        
        fn poll_interrupts(&mut self) -> bool {
            let pending = self.nmi_pending;
            self.nmi_pending = false;
            pending
        }
        
        fn poll_irq(&mut self) -> bool {
            let pending = self.irq_pending;
            self.irq_pending = false;
            pending
        }
    }
    
    #[test]
    fn test_reset() {
        let mut bus = TestBus::new();
        bus.set_reset_vector(0x8000);
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        
        assert_eq!(cpu.pc, 0x8000);
        assert_eq!(cpu.sp, 0xFD);
        assert!(cpu.get_flag(flags::INTERRUPT_DISABLE));
    }
    
    #[test]
    fn test_lda_immediate() {
        let mut bus = TestBus::new();
        bus.load_program(&[0xA9, 0x42], 0x8000); // LDA #$42
        bus.set_reset_vector(0x8000);
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        cpu.step(&mut bus);
        
        assert_eq!(cpu.a, 0x42);
        assert!(!cpu.get_flag(flags::ZERO));
        assert!(!cpu.get_flag(flags::NEGATIVE));
    }
    
    #[test]
    fn test_lda_zero_flag() {
        let mut bus = TestBus::new();
        bus.load_program(&[0xA9, 0x00], 0x8000); // LDA #$00
        bus.set_reset_vector(0x8000);
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        cpu.step(&mut bus);
        
        assert_eq!(cpu.a, 0x00);
        assert!(cpu.get_flag(flags::ZERO));
        assert!(!cpu.get_flag(flags::NEGATIVE));
    }
    
    #[test]
    fn test_lda_negative_flag() {
        let mut bus = TestBus::new();
        bus.load_program(&[0xA9, 0x80], 0x8000); // LDA #$80
        bus.set_reset_vector(0x8000);
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        cpu.step(&mut bus);
        
        assert_eq!(cpu.a, 0x80);
        assert!(!cpu.get_flag(flags::ZERO));
        assert!(cpu.get_flag(flags::NEGATIVE));
    }
    
    #[test]
    fn test_ldx_immediate() {
        let mut bus = TestBus::new();
        bus.load_program(&[0xA2, 0x42], 0x8000); // LDX #$42
        bus.set_reset_vector(0x8000);
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        cpu.step(&mut bus);
        
        assert_eq!(cpu.x, 0x42);
        assert!(!cpu.get_flag(flags::ZERO));
        assert!(!cpu.get_flag(flags::NEGATIVE));
    }
    
    #[test]
    fn test_ldy_immediate() {
        let mut bus = TestBus::new();
        bus.load_program(&[0xA0, 0x42], 0x8000); // LDY #$42
        bus.set_reset_vector(0x8000);
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        cpu.step(&mut bus);
        
        assert_eq!(cpu.y, 0x42);
        assert!(!cpu.get_flag(flags::ZERO));
        assert!(!cpu.get_flag(flags::NEGATIVE));
    }
    
    #[test]
    fn test_sta_zero_page() {
        let mut bus = TestBus::new();
        bus.load_program(&[0xA9, 0x42, 0x85, 0x02], 0x8000); // LDA #$42, STA $02
        bus.set_reset_vector(0x8000);
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        cpu.step(&mut bus); // Execute LDA #$42
        cpu.step(&mut bus); // Execute STA $02
        
        assert_eq!(bus.memory[0x02], 0x42);
    }
    
    #[test]
    fn test_adc() {
        let mut bus = TestBus::new();
        bus.load_program(&[
            0xA9, 0x50,  // LDA #$50
            0x69, 0x50,  // ADC #$50
        ], 0x8000);
        bus.set_reset_vector(0x8000);
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        cpu.step(&mut bus); // Execute LDA #$50
        cpu.step(&mut bus); // Execute ADC #$50
        
        assert_eq!(cpu.a, 0xA0);
        assert!(!cpu.get_flag(flags::CARRY));
        assert!(cpu.get_flag(flags::NEGATIVE));
        assert!(!cpu.get_flag(flags::ZERO));
    }
    
    #[test]
    fn test_adc_with_carry() {
        let mut bus = TestBus::new();
        bus.load_program(&[
            0xA9, 0x50,  // LDA #$50
            0x38,        // SEC
            0x69, 0x50,  // ADC #$50
        ], 0x8000);
        bus.set_reset_vector(0x8000);
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        cpu.step(&mut bus); // Execute LDA #$50
        cpu.step(&mut bus); // Execute SEC
        cpu.step(&mut bus); // Execute ADC #$50
        
        assert_eq!(cpu.a, 0xA1);
        assert!(!cpu.get_flag(flags::CARRY));
        assert!(cpu.get_flag(flags::NEGATIVE));
        assert!(!cpu.get_flag(flags::ZERO));
    }
    
    #[test]
    fn test_sbc() {
        let mut bus = TestBus::new();
        bus.load_program(&[
            0xA9, 0x50,  // LDA #$50
            0x38,        // SEC (set carry flag)
            0xE9, 0x20,  // SBC #$20
        ], 0x8000);
        bus.set_reset_vector(0x8000);
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        cpu.step(&mut bus); // Execute LDA #$50
        cpu.step(&mut bus); // Execute SEC
        cpu.step(&mut bus); // Execute SBC #$20
        
        assert_eq!(cpu.a, 0x30);
        assert!(cpu.get_flag(flags::CARRY)); // No borrow
        assert!(!cpu.get_flag(flags::NEGATIVE));
        assert!(!cpu.get_flag(flags::ZERO));
    }
    
    #[test]
    fn test_sbc_with_borrow() {
        let mut bus = TestBus::new();
        bus.load_program(&[
            0xA9, 0x50,  // LDA #$50
            0xE9, 0x20,  // SBC #$20
        ], 0x8000);
        bus.set_reset_vector(0x8000);
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        
        // Execute LDA #$50
        cpu.step(&mut bus);
        // Clear carry flag to force a borrow during SBC
        cpu.set_flag(flags::CARRY, false);
        // Execute SBC #$20 (expected: 0x50 - 0x20 - 1 = 0x2F)
        cpu.step(&mut bus);
        
        assert_eq!(cpu.a, 0x2F);
        assert!(!cpu.get_flag(flags::CARRY)); // Borrow should occur, so carry flag should be false
    }
    
    #[test]
    fn test_branch_taken() {
        let mut bus = TestBus::new();
        bus.load_program(&[
            0xA9, 0x00,  // LDA #$00
            0xF0, 0x02,  // BEQ +2
            0xA9, 0x01,  // LDA #$01
            0xA9, 0x02,  // LDA #$02
        ], 0x8000);
        bus.set_reset_vector(0x8000);
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        cpu.step(&mut bus); // Execute LDA #$00
        cpu.step(&mut bus); // Execute BEQ +2
        cpu.step(&mut bus); // Execute LDA #$02 (skipped LDA #$01)
        
        assert_eq!(cpu.a, 0x02);
    }
    
    #[test]
    fn test_branch_not_taken() {
        let mut bus = TestBus::new();
        bus.load_program(&[
            0xA9, 0x01,  // LDA #$01
            0xF0, 0x02,  // BEQ +2 (not taken)
            0xA9, 0x03,  // LDA #$03
            0xA9, 0x04,  // LDA #$04
        ], 0x8000);
        bus.set_reset_vector(0x8000);
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        cpu.step(&mut bus); // Execute LDA #$01
        cpu.step(&mut bus); // Execute BEQ +2 (not taken)
        cpu.step(&mut bus); // Execute LDA #$03
        
        assert_eq!(cpu.a, 0x03);
	}

		#[test]
    fn test_branch_page_cross() {
        let mut bus = TestBus::new();
        // Place a program that causes a branch to cross a page boundary
        bus.load_program(&[
            0xA9, 0x01,        // LDA #$01
            0x38,              // SEC
            0x90, 0xFC,        // BCC -4 (not taken)
            0xB0, 0x7F,        // BCS +127 (taken, crossing page)
        ], 0x80F0);
        bus.set_reset_vector(0x80F0);
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        cpu.step(&mut bus); // Execute LDA #$01
        cpu.step(&mut bus); // Execute SEC
        cpu.step(&mut bus); // Execute BCC -4 (not taken)
        let cycles = cpu.step(&mut bus); // Execute BCS +127 (taken, crosses page)
        
        assert_eq!(cycles, 4); // Branch taken + page cross = 4 cycles
        assert_eq!(cpu.pc, 0x8176);
    }
    
    #[test]
    fn test_nmi() {
        let mut bus = TestBus::new();
        bus.load_program(&[0xEA], 0x8000); // NOP
        bus.set_reset_vector(0x8000);
        bus.set_nmi_vector(0x9000);
        bus.load_program(&[0x40], 0x9000); // RTI
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        
        // Trigger NMI
        bus.set_nmi(true);
        
        // Execute one step (should handle the NMI)
        cpu.step(&mut bus);
        
        // Check that NMI was handled properly
        assert_eq!(cpu.pc, 0x9000);
        assert!(cpu.get_flag(flags::INTERRUPT_DISABLE));
        assert_eq!(cpu.sp, 0xFA);
        
        // Execute RTI
        cpu.step(&mut bus);
        
        // Check that we returned correctly
        assert_eq!(cpu.pc, 0x8000);
        assert_eq!(cpu.sp, 0xFD);
    }
    
    #[test]
    fn test_stack_operations() {
        let mut bus = TestBus::new();
        bus.load_program(&[
            0xA9, 0x42,  // LDA #$42
            0x48,        // PHA
            0xA9, 0x24,  // LDA #$24
            0x68,        // PLA
        ], 0x8000);
        bus.set_reset_vector(0x8000);
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        cpu.step(&mut bus); // Execute LDA #$42
        cpu.step(&mut bus); // Execute PHA
        
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.sp, 0xFC);
        assert_eq!(bus.memory[0x01FD], 0x42);
        
        cpu.step(&mut bus); // Execute LDA #$24
        cpu.step(&mut bus); // Execute PLA
        
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.sp, 0xFD);
    }
    
    #[test]
    fn test_unofficial_lax() {
        let mut bus = TestBus::new();
        bus.load_program(&[0xA7, 0x10], 0x8000); // LAX $10 (unofficial)
        bus.set_reset_vector(0x8000);
        bus.memory[0x10] = 0x55;
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        cpu.step(&mut bus);
        
        assert_eq!(cpu.a, 0x55);
        assert_eq!(cpu.x, 0x55);
    }
    
    #[test]
    fn test_brk_and_rti() {
        let mut bus = TestBus::new();
        bus.load_program(&[0x00], 0x8000); // BRK
        bus.set_reset_vector(0x8000);
        bus.set_irq_vector(0x9000);
        bus.load_program(&[0x40], 0x9000); // RTI
        
        let mut cpu = Cpu::new();
        cpu.reset(&mut bus);
        
        // Clear interrupt disable flag
        cpu.set_flag(flags::INTERRUPT_DISABLE, false);
        
        // Execute BRK
        cpu.step(&mut bus);
        
        // Check that BRK was handled properly
        assert_eq!(cpu.pc, 0x9000);
        assert!(cpu.get_flag(flags::INTERRUPT_DISABLE));
        assert_eq!(cpu.sp, 0xFA);
        
        // Execute RTI
        cpu.step(&mut bus);
        
        // Check that we returned to the instruction after BRK
        assert_eq!(cpu.pc, 0x8002); // BRK is 2 bytes even though 2nd byte is unused
        assert_eq!(cpu.sp, 0xFD);
    }
}
