//! CPU implementation for the Ricoh 2A03 (modified MOS 6502)
//!
//! The 2A03 is a MOS 6502 modified for the NES, with the following changes:
//! - Decimal mode is disabled (D flag is ignored)
//! - Contains the APU (Audio Processing Unit)
//!
//! This implementation focuses on cycle-accurate timing to ensure proper
//! synchronization with other components of the NES.

use log::{debug, trace};
use crate::memory::MemoryBus;

/// Status register flag bits
#[allow(dead_code)]
pub mod flags {
    pub const CARRY: u8 = 0x01;
    pub const ZERO: u8 = 0x02;
    pub const INTERRUPT_DISABLE: u8 = 0x04;
    pub const DECIMAL: u8 = 0x08;  // Ignored on 2A03, but still settable
    pub const BREAK: u8 = 0x10;
    pub const UNUSED: u8 = 0x20;   // Always set to 1
    pub const OVERFLOW: u8 = 0x40;
    pub const NEGATIVE: u8 = 0x80;
}

/// Addressing modes for CPU instructions
#[derive(Debug, Copy, Clone, PartialEq)]
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
    IndexedIndirect,  // (Indirect,X)
    IndirectIndexed,  // (Indirect),Y
}

/// Represents the Ricoh 2A03 CPU
pub struct CPU {
    /// Accumulator register
    pub a: u8,
    /// X index register
    pub x: u8,
    /// Y index register
    pub y: u8,
    /// Stack pointer (0x0100 - 0x01FF)
    pub sp: u8,
    /// Program counter
    pub pc: u16,
    /// Status register
    pub p: u8,
    /// Cycle count for the last instruction
    pub cycles: u8,
    /// Total cycles executed
    pub total_cycles: u64,
    /// Whether the CPU is waiting for an interrupt
    pub waiting: bool,
}

impl CPU {
    /// Create a new CPU in the reset state
    pub fn new() -> Self {
        CPU {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,  // Initial stack pointer value after reset
            pc: 0,     // Will be initialized from reset vector
            p: flags::UNUSED | flags::INTERRUPT_DISABLE,  // Initial status after reset
            cycles: 0,
            total_cycles: 0,
            waiting: false,
        }
    }

    /// Reset the CPU to its initial state
    pub fn reset(&mut self) {
        self.a = 0;
        self.x = 0;
        self.y = 0;
        self.sp = 0xFD;
        self.p = flags::UNUSED | flags::INTERRUPT_DISABLE;
        self.cycles = 0;
        self.total_cycles = 0;
        self.waiting = false;
        
        // The PC will be set from the reset vector during the first execution cycle
    }

    /// Execute a single CPU instruction and return the number of cycles used
    pub fn step(&mut self, bus: &mut MemoryBus) -> u32 {
        // If this is the first execution (PC = 0), read the reset vector
        if self.pc == 0 {
            let low = bus.read(0xFFFC);
            let high = bus.read(0xFFFD);
            self.pc = u16::from_le_bytes([low, high]);
            debug!("CPU reset to ${:04X}", self.pc);
            return 7; // Reset takes 7 cycles
        }
        
        // Handle interrupts
        if self.check_interrupts(bus) {
            return self.cycles as u32;
        }
        
        // Fetch instruction
        let opcode = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        
        // Execute instruction
        trace!("CPU: ${:04X}: ${:02X} A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X}",
              self.pc.wrapping_sub(1), opcode, self.a, self.x, self.y, self.p, self.sp);
        
        // Decode and execute the instruction
        self.execute_instruction(opcode, bus);
        
        // Convert cycles to u32 for return value
        let cycles_used = self.cycles as u32;
        self.total_cycles += cycles_used as u64;
        
        cycles_used
    }

    /// Check for and process any pending interrupts
    fn check_interrupts(&mut self, bus: &mut MemoryBus) -> bool {
        // NMI has highest priority
        if bus.peek_nmi() {
            self.handle_nmi(bus);
            return true;
        }
        
        // IRQ is next if interrupts are enabled
        if bus.peek_irq() && (self.p & flags::INTERRUPT_DISABLE) == 0 {
            self.handle_irq(bus);
            return true;
        }
        
        false
    }

    /// Handle a non-maskable interrupt (NMI)
    fn handle_nmi(&mut self, bus: &mut MemoryBus) {
        bus.acknowledge_nmi();
        
        // Push PC and processor status to stack
        self.push_word(bus, self.pc);
        self.push_byte(bus, self.p & !flags::BREAK);
        
        // Set the interrupt flag
        self.p |= flags::INTERRUPT_DISABLE;
        
        // Load the NMI vector
        let low = bus.read(0xFFFA);
        let high = bus.read(0xFFFB);
        self.pc = u16::from_le_bytes([low, high]);
        
        // NMI takes 7 cycles
        self.cycles = 7;
        
        debug!("NMI handled, jumping to ${:04X}", self.pc);
    }

    /// Handle an interrupt request (IRQ)
    fn handle_irq(&mut self, bus: &mut MemoryBus) {
        bus.acknowledge_irq();
        
        // Push PC and processor status to stack
        self.push_word(bus, self.pc);
        self.push_byte(bus, self.p & !flags::BREAK);
        
        // Set the interrupt flag
        self.p |= flags::INTERRUPT_DISABLE;
        
        // Load the IRQ vector
        let low = bus.read(0xFFFE);
        let high = bus.read(0xFFFF);
        self.pc = u16::from_le_bytes([low, high]);
        
        // IRQ takes 7 cycles
        self.cycles = 7;
        
        debug!("IRQ handled, jumping to ${:04X}", self.pc);
    }

    /// Push a byte onto the stack
    fn push_byte(&mut self, bus: &mut MemoryBus, value: u8) {
        bus.write(0x0100 + u16::from(self.sp), value);
        self.sp = self.sp.wrapping_sub(1);
    }

    /// Push a word (2 bytes) onto the stack
    fn push_word(&mut self, bus: &mut MemoryBus, value: u16) {
        let [low, high] = value.to_le_bytes();
        self.push_byte(bus, high);
        self.push_byte(bus, low);
    }

    /// Pop a byte from the stack
    fn pop_byte(&mut self, bus: &mut MemoryBus) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        bus.read(0x0100 + u16::from(self.sp))
    }

    /// Pop a word (2 bytes) from the stack
    fn pop_word(&mut self, bus: &mut MemoryBus) -> u16 {
        let low = self.pop_byte(bus);
        let high = self.pop_byte(bus);
        u16::from_le_bytes([low, high])
    }

    /// Get the address for the given addressing mode
    fn get_address(&mut self, mode: AddressingMode, bus: &mut MemoryBus) -> u16 {
        match mode {
            AddressingMode::Implied | AddressingMode::Accumulator => {
                0 // Not used for these modes
            }
            AddressingMode::Immediate => {
                let addr = self.pc;
                self.pc = self.pc.wrapping_add(1);
                addr
            }
            AddressingMode::ZeroPage => {
                let addr = bus.read(self.pc) as u16;
                self.pc = self.pc.wrapping_add(1);
                addr
            }
            AddressingMode::ZeroPageX => {
                let base = bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                (base.wrapping_add(self.x)) as u16
            }
            AddressingMode::ZeroPageY => {
                let base = bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                (base.wrapping_add(self.y)) as u16
            }
            AddressingMode::Relative => {
                let offset = bus.read(self.pc) as i8;
                self.pc = self.pc.wrapping_add(1);
                self.pc.wrapping_add(offset as u16)
            }
            AddressingMode::Absolute => {
                let low = bus.read(self.pc);
                let high = bus.read(self.pc.wrapping_add(1));
                self.pc = self.pc.wrapping_add(2);
                u16::from_le_bytes([low, high])
            }
            AddressingMode::AbsoluteX => {
                let low = bus.read(self.pc);
                let high = bus.read(self.pc.wrapping_add(1));
                self.pc = self.pc.wrapping_add(2);
                let base = u16::from_le_bytes([low, high]);
                base.wrapping_add(self.x as u16)
            }
            AddressingMode::AbsoluteY => {
                let low = bus.read(self.pc);
                let high = bus.read(self.pc.wrapping_add(1));
                self.pc = self.pc.wrapping_add(2);
                let base = u16::from_le_bytes([low, high]);
                base.wrapping_add(self.y as u16)
            }
            AddressingMode::Indirect => {
                let low = bus.read(self.pc);
                let high = bus.read(self.pc.wrapping_add(1));
                self.pc = self.pc.wrapping_add(2);
                let ptr = u16::from_le_bytes([low, high]);
                
                // Replicate 6502 indirect JMP bug for page crossing
                let target_low = bus.read(ptr);
                let target_high = if low == 0xFF {
                    bus.read(ptr & 0xFF00)
                } else {
                    bus.read(ptr.wrapping_add(1))
                };
                
                u16::from_le_bytes([target_low, target_high])
            }
            AddressingMode::IndexedIndirect => {
                let base = bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                let ptr = base.wrapping_add(self.x) as u16;
                
                let low = bus.read(ptr);
                let high = bus.read(ptr.wrapping_add(1) & 0xFF);
                
                u16::from_le_bytes([low, high])
            }
            AddressingMode::IndirectIndexed => {
                let base = bus.read(self.pc) as u16;
                self.pc = self.pc.wrapping_add(1);
                
                let low = bus.read(base);
                let high = bus.read((base + 1) & 0xFF);
                
                let addr = u16::from_le_bytes([low, high]);
                addr.wrapping_add(self.y as u16)
            }
        }
    }

    /// Execute an instruction with the given opcode
    fn execute_instruction(&mut self, opcode: u8, bus: &mut MemoryBus) {
        match opcode {
            // This is a simplified implementation with just a few common instructions
            // In a complete implementation, all 151 valid 6502 opcodes should be handled
            
            // LDA - Load Accumulator
            0xA9 => self.lda(AddressingMode::Immediate, bus),
            0xA5 => self.lda(AddressingMode::ZeroPage, bus),
            0xB5 => self.lda(AddressingMode::ZeroPageX, bus),
            0xAD => self.lda(AddressingMode::Absolute, bus),
            0xBD => self.lda(AddressingMode::AbsoluteX, bus),
            0xB9 => self.lda(AddressingMode::AbsoluteY, bus),
            0xA1 => self.lda(AddressingMode::IndexedIndirect, bus),
            0xB1 => self.lda(AddressingMode::IndirectIndexed, bus),

            // LDX - Load X Register
            0xA2 => self.ldx(AddressingMode::Immediate, bus),
            0xA6 => self.ldx(AddressingMode::ZeroPage, bus),
            0xB6 => self.ldx(AddressingMode::ZeroPageY, bus),
            0xAE => self.ldx(AddressingMode::Absolute, bus),
            0xBE => self.ldx(AddressingMode::AbsoluteY, bus),
            
            // LDY - Load Y Register
            0xA0 => self.ldy(AddressingMode::Immediate, bus),
            0xA4 => self.ldy(AddressingMode::ZeroPage, bus),
            0xB4 => self.ldy(AddressingMode::ZeroPageX, bus),
            0xAC => self.ldy(AddressingMode::Absolute, bus),
            0xBC => self.ldy(AddressingMode::AbsoluteX, bus),
            
            // STA - Store Accumulator
            0x85 => self.sta(AddressingMode::ZeroPage, bus),
            0x95 => self.sta(AddressingMode::ZeroPageX, bus),
            0x8D => self.sta(AddressingMode::Absolute, bus),
            0x9D => self.sta(AddressingMode::AbsoluteX, bus),
            0x99 => self.sta(AddressingMode::AbsoluteY, bus),
            0x81 => self.sta(AddressingMode::IndexedIndirect, bus),
            0x91 => self.sta(AddressingMode::IndirectIndexed, bus),
            
            // STX - Store X Register
            0x86 => self.stx(AddressingMode::ZeroPage, bus),
            0x96 => self.stx(AddressingMode::ZeroPageY, bus),
            0x8E => self.stx(AddressingMode::Absolute, bus),
            
            // STY - Store Y Register
            0x84 => self.sty(AddressingMode::ZeroPage, bus),
            0x94 => self.sty(AddressingMode::ZeroPageX, bus),
            0x8C => self.sty(AddressingMode::Absolute, bus),
            
            // JMP - Jump
            0x4C => self.jmp(AddressingMode::Absolute, bus),
            0x6C => self.jmp(AddressingMode::Indirect, bus),
            
            // JSR - Jump to Subroutine
            0x20 => self.jsr(bus),
            
            // RTS - Return from Subroutine
            0x60 => self.rts(bus),
            
            // BCC - Branch if Carry Clear
            0x90 => self.bcc(bus),
            
            // BCS - Branch if Carry Set
            0xB0 => self.bcs(bus),
            
            // BEQ - Branch if Equal (Zero Set)
            0xF0 => self.beq(bus),
            
            // BNE - Branch if Not Equal (Zero Clear)
            0xD0 => self.bne(bus),
            
            // BVC - Branch if Overflow Clear
            0x50 => self.bvc(bus),
            
            // BVS - Branch if Overflow Set
            0x70 => self.bvs(bus),
            
            // BPL - Branch if Plus (Negative Clear)
            0x10 => self.bpl(bus),
            
            // BMI - Branch if Minus (Negative Set)
            0x30 => self.bmi(bus),
            
            // CLC - Clear Carry Flag
            0x18 => {
                self.p &= !flags::CARRY;
                self.cycles = 2;
            },
            
            // SEC - Set Carry Flag
            0x38 => {
                self.p |= flags::CARRY;
                self.cycles = 2;
            },
            
            // CLI - Clear Interrupt Disable
            0x58 => {
                self.p &= !flags::INTERRUPT_DISABLE;
                self.cycles = 2;
            },
            
            // SEI - Set Interrupt Disable
            0x78 => {
                self.p |= flags::INTERRUPT_DISABLE;
                self.cycles = 2;
            },
            
            // CLD - Clear Decimal Mode
            0xD8 => {
                self.p &= !flags::DECIMAL;
                self.cycles = 2;
                // Note: On the 2A03, the decimal flag can be set but decimal mode
                // does not function, as it was disabled by Nintendo.
            },
            
            // SED - Set Decimal Mode
            0xF8 => {
                self.p |= flags::DECIMAL;
                self.cycles = 2;
                // Note: On the 2A03, the decimal flag can be set but decimal mode
                // does not function, as it was disabled by Nintendo.
            },
            
            // CLV - Clear Overflow Flag
            0xB8 => {
                self.p &= !flags::OVERFLOW;
                self.cycles = 2;
            },
            
            // NOP - No Operation
            0xEA => {
                self.cycles = 2;
            },

            // This is a simplified instruction set for brevity.
            // In a complete implementation, all 151 valid opcodes would be handled here.
            // The remaining instructions (ADC, SBC, AND, ORA, EOR, etc.) would follow
            // similar patterns to those shown above.
            
            _ => {
                // Illegal/unimplemented opcode
                debug!("Unimplemented opcode: ${:02X} at ${:04X}", opcode, self.pc - 1);
                self.cycles = 2; // Default to 2 cycles
            }
        }
    }

    // Implementation of individual instructions
    
    /// LDA - Load Accumulator
    fn lda(&mut self, mode: AddressingMode, bus: &mut MemoryBus) {
        let addr = self.get_address(mode, bus);
        let value = if mode == AddressingMode::Immediate {
            bus.read(addr)
        } else {
            bus.read(addr)
        };
        
        self.a = value;
        
        // Set zero and negative flags
        self.p = (self.p & !(flags::ZERO | flags::NEGATIVE))
            | if self.a == 0 { flags::ZERO } else { 0 }
            | (self.a & flags::NEGATIVE);
        
        // Set cycles based on addressing mode
        self.cycles = match mode {
            AddressingMode::Immediate => 2,
            AddressingMode::ZeroPage => 3,
            AddressingMode::ZeroPageX => 4,
            AddressingMode::Absolute => 4,
            AddressingMode::AbsoluteX | AddressingMode::AbsoluteY => {
                // Add 1 cycle if page boundary is crossed
                if (addr & 0xFF00) != ((addr - (if mode == AddressingMode::AbsoluteX { self.x } else { self.y }) as u16) & 0xFF00) {
                    5
                } else {
                    4
                }
            },
            AddressingMode::IndexedIndirect => 6,
            AddressingMode::IndirectIndexed => {
                // Add 1 cycle if page boundary is crossed
                let base_addr = self.get_address(AddressingMode::ZeroPage, bus);
                let indirect_addr = u16::from_le_bytes([
                    bus.read(base_addr),
                    bus.read((base_addr + 1) & 0xFF),
                ]);
                
                if (addr & 0xFF00) != (indirect_addr & 0xFF00) {
                    6
                } else {
                    5
                }
            },
            _ => panic!("Invalid addressing mode for LDA: {:?}", mode),
        };
    }

    /// LDX - Load X Register
    fn ldx(&mut self, mode: AddressingMode, bus: &mut MemoryBus) {
        let addr = self.get_address(mode, bus);
        let value = if mode == AddressingMode::Immediate {
            bus.read(addr)
        } else {
            bus.read(addr)
        };
        
        self.x = value;
        
        // Set zero and negative flags
        self.p = (self.p & !(flags::ZERO | flags::NEGATIVE))
            | if self.x == 0 { flags::ZERO } else { 0 }
            | (self.x & flags::NEGATIVE);
        
        // Set cycles based on addressing mode
        self.cycles = match mode {
            AddressingMode::Immediate => 2,
            AddressingMode::ZeroPage => 3,
            AddressingMode::ZeroPageY => 4,
            AddressingMode::Absolute => 4,
            AddressingMode::AbsoluteY => {
                // Add 1 cycle if page boundary is crossed
                if (addr & 0xFF00) != ((addr - self.y as u16) & 0xFF00) {
                    5
                } else {
                    4
                }
            },
            _ => panic!("Invalid addressing mode for LDX: {:?}", mode),
        };
    }

    /// LDY - Load Y Register
    fn ldy(&mut self, mode: AddressingMode, bus: &mut MemoryBus) {
        let addr = self.get_address(mode, bus);
        let value = if mode == AddressingMode::Immediate {
            bus.read(addr)
        } else {
            bus.read(addr)
        };
        
        self.y = value;
        
        // Set zero and negative flags
        self.p = (self.p & !(flags::ZERO | flags::NEGATIVE))
            | if self.y == 0 { flags::ZERO } else { 0 }
            | (self.y & flags::NEGATIVE);
        
        // Set cycles based on addressing mode
        self.cycles = match mode {
            AddressingMode::Immediate => 2,
            AddressingMode::ZeroPage => 3,
            AddressingMode::ZeroPageX => 4,
            AddressingMode::Absolute => 4,
            AddressingMode::AbsoluteX => {
                // Add 1 cycle if page boundary is crossed
                if (addr & 0xFF00) != ((addr - self.x as u16) & 0xFF00) {
                    5
                } else {
                    4
                }
            },
            _ => panic!("Invalid addressing mode for LDY: {:?}", mode),
        };
    }

    /// STA - Store Accumulator
    fn sta(&mut self, mode: AddressingMode, bus: &mut MemoryBus) {
        let addr = self.get_address(mode, bus);
        bus.write(addr, self.a);
        
        // Set cycles based on addressing mode
        self.cycles = match mode {
            AddressingMode::ZeroPage => 3,
            AddressingMode::ZeroPageX => 4,
            AddressingMode::Absolute => 4,
            AddressingMode::AbsoluteX => 5,
            AddressingMode::AbsoluteY => 5,
            AddressingMode::IndexedIndirect => 6,
            AddressingMode::IndirectIndexed => 6,
            _ => panic!("Invalid addressing mode for STA: {:?}", mode),
        };
    }

    /// STX - Store X Register
    fn stx(&mut self, mode: AddressingMode, bus: &mut MemoryBus) {
        let addr = self.get_address(mode, bus);
        bus.write(addr, self.x);
        
        // Set cycles based on addressing mode
        self.cycles = match mode {
            AddressingMode::ZeroPage => 3,
            AddressingMode::ZeroPageY => 4,
            AddressingMode::Absolute => 4,
            _ => panic!("Invalid addressing mode for STX: {:?}", mode),
        };
    }

    /// STY - Store Y Register
    fn sty(&mut self, mode: AddressingMode, bus: &mut MemoryBus) {
        let addr = self.get_address(mode, bus);
        bus.write(addr, self.y);
        
        // Set cycles based on addressing mode
        self.cycles = match mode {
            AddressingMode::ZeroPage => 3,
            AddressingMode::ZeroPageX => 4,
            AddressingMode::Absolute => 4,
            _ => panic!("Invalid addressing mode for STY: {:?}", mode),
        };
    }

    /// JMP - Jump
    fn jmp(&mut self, mode: AddressingMode, bus: &mut MemoryBus) {
        let addr = self.get_address(mode, bus);
        self.pc = addr;
        
        // Set cycles based on addressing mode
        self.cycles = match mode {
            AddressingMode::Absolute => 3,
            AddressingMode::Indirect => 5,
            _ => panic!("Invalid addressing mode for JMP: {:?}", mode),
        };
    }

    /// JSR - Jump to Subroutine
    fn jsr(&mut self, bus: &mut MemoryBus) {
        let target = self.get_address(AddressingMode::Absolute, bus);
        
        // Push the return address (PC - 1) to the stack
        self.push_word(bus, self.pc.wrapping_sub(1));
        
        // Jump to the target address
        self.pc = target;
        
        // JSR takes 6 cycles
        self.cycles = 6;
    }

    /// RTS - Return from Subroutine
    fn rts(&mut self, bus: &mut MemoryBus) {
        // Pop the return address from the stack
        let addr = self.pop_word(bus);
        
        // Set PC to the return address + 1
        self.pc = addr.wrapping_add(1);
        
        // RTS takes 6 cycles
        self.cycles = 6;
    }

    /// Branch instructions
    
    /// BCC - Branch if Carry Clear
    fn bcc(&mut self, bus: &mut MemoryBus) {
        let target = self.get_address(AddressingMode::Relative, bus);
        
        // Check if the carry flag is clear
        if (self.p & flags::CARRY) == 0 {
            // Branch taken - additional cycle
            self.cycles = 3;
            
            // Check if page boundary is crossed
            if (self.pc & 0xFF00) != (target & 0xFF00) {
                self.cycles += 1;
            }
            
            self.pc = target;
        } else {
            // Branch not taken
            self.cycles = 2;
        }
    }

    /// BCS - Branch if Carry Set
    fn bcs(&mut self, bus: &mut MemoryBus) {
        let target = self.get_address(AddressingMode::Relative, bus);
        
        // Check if the carry flag is set
        if (self.p & flags::CARRY) != 0 {
            // Branch taken - additional cycle
            self.cycles = 3;
            
            // Check if page boundary is crossed
            if (self.pc & 0xFF00) != (target & 0xFF00) {
                self.cycles += 1;
            }
            
            self.pc = target;
        } else {
            // Branch not taken
            self.cycles = 2;
        }
    }

    /// BEQ - Branch if Equal (Zero Set)
    fn beq(&mut self, bus: &mut MemoryBus) {
        let target = self.get_address(AddressingMode::Relative, bus);
        
        // Check if the zero flag is set
        if (self.p & flags::ZERO) != 0 {
            // Branch taken - additional cycle
            self.cycles = 3;
            
            // Check if page boundary is crossed
            if (self.pc & 0xFF00) != (target & 0xFF00) {
                self.cycles += 1;
            }
            
            self.pc = target;
        } else {
            // Branch not taken
            self.cycles = 2;
        }
    }

    /// BNE - Branch if Not Equal (Zero Clear)
    fn bne(&mut self, bus: &mut MemoryBus) {
        let target = self.get_address(AddressingMode::Relative, bus);
        
        // Check if the zero flag is clear
        if (self.p & flags::ZERO) == 0 {
            // Branch taken - additional cycle
            self.cycles = 3;
            
            // Check if page boundary is crossed
            if (self.pc & 0xFF00) != (target & 0xFF00) {
                self.cycles += 1;
            }
            
            self.pc = target;
        } else {
            // Branch not taken
            self.cycles = 2;
        }
    }

    /// BVC - Branch if Overflow Clear
    fn bvc(&mut self, bus: &mut MemoryBus) {
        let target = self.get_address(AddressingMode::Relative, bus);
        
        // Check if the overflow flag is clear
        if (self.p & flags::OVERFLOW) == 0 {
            // Branch taken - additional cycle
            self.cycles = 3;
            
            // Check if page boundary is crossed
            if (self.pc & 0xFF00) != (target & 0xFF00) {
                self.cycles += 1;
            }
            
            self.pc = target;
        } else {
            // Branch not taken
            self.cycles = 2;
        }
    }

    /// BVS - Branch if Overflow Set
    fn bvs(&mut self, bus: &mut MemoryBus) {
        let target = self.get_address(AddressingMode::Relative, bus);
        
        // Check if the overflow flag is set
        if (self.p & flags::OVERFLOW) != 0 {
            // Branch taken - additional cycle
            self.cycles = 3;
            
            // Check if page boundary is crossed
            if (self.pc & 0xFF00) != (target & 0xFF00) {
                self.cycles += 1;
            }
            
            self.pc = target;
        } else {
            // Branch not taken
            self.cycles = 2;
        }
    }

    /// BPL - Branch if Plus (Negative Clear)
    fn bpl(&mut self, bus: &mut MemoryBus) {
        let target = self.get_address(AddressingMode::Relative, bus);
        
        // Check if the negative flag is clear
        if (self.p & flags::NEGATIVE) == 0 {
            // Branch taken - additional cycle
            self.cycles = 3;
            
            // Check if page boundary is crossed
            if (self.pc & 0xFF00) != (target & 0xFF00) {
                self.cycles += 1;
            }
            
            self.pc = target;
        } else {
            // Branch not taken
            self.cycles = 2;
        }
    }

    /// BMI - Branch if Minus (Negative Set)
    fn bmi(&mut self, bus: &mut MemoryBus) {
        let target = self.get_address(AddressingMode::Relative, bus);
        
        // Check if the negative flag is set
        if (self.p & flags::NEGATIVE) != 0 {
            // Branch taken - additional cycle
            self.cycles = 3;
            
            // Check if page boundary is crossed
            if (self.pc & 0xFF00) != (target & 0xFF00) {
                self.cycles += 1;
            }
            
            self.pc = target;
        } else {
            // Branch not taken
            self.cycles = 2;
        }
    }
}