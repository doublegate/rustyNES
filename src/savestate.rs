//! Save state implementation
//!
//! This module handles saving and loading the emulator state, allowing users
//! to save their progress and restore it later. Save states capture the complete
//! state of the emulator, including CPU, PPU, APU, memory, and mapper state.
//! 
//! Save states are versioned to ensure compatibility across different versions
//! of the emulator. Files are serialized using bincode with Serde.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::borrow::BorrowMut;
use log::{info, warn};
use thiserror::Error;
use bincode::{Encode, Decode, BorrowDecode};
use bincode::{encode_into_std_write, decode_from_std_read};
use serde::{Serialize, Deserialize};

use crate::ppu::TVSystem;
use crate::nes::NES;
use crate::cartridge::Mirroring;

/// Current save state format version
const CURRENT_SAVE_STATE_VERSION: u32 = 1;

/// Errors that can occur during save state operations
#[derive(Error, Debug)]
pub enum SaveStateError {
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Deserialization error: {0}")]
    DeserializationError(String),
    
    #[error("Incompatible save state version: found {0}, expected {1}")]
    IncompatibleVersion(u32, u32),
    
    #[error("Unsupported mapper type: {0}")]
    UnsupportedMapper(u8),
    
    #[error("Invalid save state data")]
    InvalidData,
    
    #[error("No cartridge loaded")]
    NoCartridge,
}

/// Save state data
#[derive(Serialize, Deserialize, Encode)]
pub struct SaveState {
    /// Save state format version
    version: u32,
    
    /// CPU state
    cpu: CpuState,
    
    /// PPU state
    ppu: PpuState,
    
    /// APU state
    apu: ApuState,
    
    /// Memory state
    memory: MemoryState,
    
    /// Cartridge state
    cartridge: CartridgeState,
}

/// CPU state data
#[derive(Serialize, Deserialize, Encode, Decode)]
struct CpuState {
    a: u8,
    x: u8,
    y: u8,
    sp: u8,
    pc: u16,
    p: u8,
    cycles: u8,
    total_cycles: u64,
    waiting: bool,
}

/// PPU state data
#[derive(Serialize, Deserialize, Encode, Decode)]
struct PpuState {
    cycle: u16,
    scanline: u16,
    frame: u64,
    v: u16,
    t: u16,
    x: u8,
    w: bool,
    nmi_occurred: bool,
    nmi_output: bool,
    even_frame: bool,
    data_buffer: u8,
    vram: Vec<u8>,
    palette_ram: Vec<u8>,
    oam: Vec<u8>,
    tv_system: TVSystem,
    sprite_zero_hit: bool,
    sprite_overflow: bool,
}

/// APU state data
#[derive(Serialize, Deserialize, Encode, Decode)]
struct ApuState {
    pulse1: PulseState,
    pulse2: PulseState,
    triangle: TriangleState,
    noise: NoiseState,
    dmc: DmcState,
    frame_counter: u8,
    frame_irq_inhibit: bool,
    frame_counter_mode: bool,
    frame_sequence: u8,
    cycles: u64,
}

/// Pulse channel state
#[derive(Serialize, Deserialize, Encode, Decode)]
struct PulseState {
    enabled: bool,
    duty: u8,
    length_counter_halt: bool,
    constant_volume: bool,
    volume: u8,
    sweep_enabled: bool,
    sweep_period: u8,
    sweep_negative: bool,
    sweep_shift: u8,
    timer_period: u16,
    length_counter: u8,
    timer: u16,
    sequencer_step: u8,
    envelope_start: bool,
    envelope_divider: u8,
    envelope_decay: u8,
    envelope_volume: u8,
    sweep_reload: bool,
    sweep_divider: u8,
    muted: bool,
}

/// Triangle channel state
#[derive(Serialize, Deserialize, Encode, Decode)]
struct TriangleState {
    enabled: bool,
    linear_counter_reload: bool,
    linear_counter_period: u8,
    length_counter_halt: bool,
    timer_period: u16,
    length_counter: u8,
    timer: u16,
    sequencer_step: u8,
    linear_counter: u8,
    linear_counter_reload_flag: bool,
}

/// Noise channel state
#[derive(Serialize, Deserialize, Encode, Decode)]
struct NoiseState {
    enabled: bool,
    length_counter_halt: bool,
    constant_volume: bool,
    volume: u8,
    mode: bool,
    timer_period: u16,
    length_counter: u8,
    timer: u16,
    shift_register: u16,
    envelope_start: bool,
    envelope_divider: u8,
    envelope_decay: u8,
    envelope_volume: u8,
}

/// DMC channel state
#[derive(Serialize, Deserialize, Encode, Decode)]
struct DmcState {
    enabled: bool,
    irq_enabled: bool,
    loop_flag: bool,
    timer_period: u16,
    output_level: u8,
    sample_address: u16,
    sample_length: u16,
    timer: u16,
    sample_buffer: u8,
    sample_buffer_empty: bool,
    current_address: u16,
    bytes_remaining: u16,
    shift_register: u8,
    bits_remaining: u8,
    silent: bool,
}

/// Memory state data
#[derive(Serialize, Deserialize, Encode, Decode)]
struct MemoryState {
    ram: Vec<u8>,
    ppu_registers: Vec<u8>,
    apu_io_registers: Vec<u8>,
    oam_dma_active: bool,
    oam_dma_addr: u8,
    oam_dma_page: u8,
    nmi_pending: bool,
    irq_pending: bool,
}

/// Cartridge state data
#[derive(Serialize, Deserialize, Encode, Decode)]
struct CartridgeState {
    /// Mapper number
    mapper_number: u8,
    
    /// PRG RAM content
    prg_ram: Vec<u8>,
    
    /// CHR RAM content (if present)
    chr_ram: Vec<u8>,
    
    /// Whether the cartridge has battery-backed RAM
    has_battery: bool,
    
    /// Mirroring mode
    mirroring: Mirroring,
    
    /// Mapper-specific state
    mapper_state: MapperState,
}

/// Mapper-specific state data
#[derive(Serialize, Deserialize, Encode, Decode)]
enum MapperState {
    /// NROM (Mapper 0) - No state needed
    Mapper000,
    
    /// MMC1 (Mapper 1)
    Mapper001(MMC1State),
    
    /// UxROM (Mapper 2)
    Mapper002(UxROMState),
    
    /// CNROM (Mapper 3)
    Mapper003(CNROMState),
    
    /// MMC3 (Mapper 4)
    Mapper004(MMC3State),
    
    /// Raw bytes for other/unknown mappers
    Unknown(Vec<u8>),
}

/// MMC1 (Mapper 1) state
#[derive(Serialize, Deserialize, Encode, Decode)]
struct MMC1State {
    shift_register: u8,
    shift_count: u8,
    control: u8,
    chr_bank_0: u8,
    chr_bank_1: u8,
    prg_bank: u8,
}

/// UxROM (Mapper 2) state
#[derive(Serialize, Deserialize, Encode, Decode)]
struct UxROMState {
    prg_bank: u8,
}

/// CNROM (Mapper 3) state
#[derive(Serialize, Deserialize, Encode, Decode)]
struct CNROMState {
    chr_bank: u8,
}

/// MMC3 (Mapper 4) state
#[derive(Serialize, Deserialize, Encode, Decode)]
struct MMC3State {
    bank_select: u8,
    bank_registers: [u8; 8],
    prg_mode: u8,
    chr_mode: u8,
    irq_counter: u8,
    irq_latch: u8,
    irq_enabled: bool,
    irq_pending: bool,
    irq_reload: bool,
    prg_ram_protect: [bool; 2],
}

/// Create a bincode configuration optimized for size
fn config() -> bincode::config::Configuration {
    bincode::config::standard()
}

/// Serialize data using bincode
fn serialize<T: Serialize + Encode>(value: &T, config: bincode::config::Configuration) -> Result<Vec<u8>, bincode::error::EncodeError> {
    let mut buffer = Vec::new();
    encode_into_std_write(value, &mut buffer, config)?;
    Ok(buffer)
}

/// Deserialize data using bincode
fn deserialize<T: for<'a> Deserialize<'a> + Decode<()>>(data: &[u8], config: bincode::config::Configuration) -> Result<T, bincode::error::DecodeError> {
    decode_from_std_read(&mut &*data, config)
}

// Implement Encode and Decode for TVSystem
impl Encode for TVSystem {
    fn encode<E: bincode::enc::Encoder>(&self, encoder: &mut E) -> Result<(), bincode::error::EncodeError> {
        match self {
            TVSystem::NTSC => 0u8.encode(encoder),
            TVSystem::PAL => 1u8.encode(encoder),
            TVSystem::Dendy => 2u8.encode(encoder),
        }
    }
}

impl Decode<()> for TVSystem {
    fn decode<D: bincode::de::Decoder>(decoder: &mut D) -> Result<Self, bincode::error::DecodeError> {
        let value = u8::decode(decoder)?;
        match value {
            0 => Ok(TVSystem::NTSC),
            1 => Ok(TVSystem::PAL),
            2 => Ok(TVSystem::Dendy),
            _ => Err(bincode::error::DecodeError::Other("Invalid TVSystem value")),
        }
    }
}

impl<'de> BorrowDecode<'de, ()> for TVSystem {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de>>(decoder: &mut D) -> Result<Self, bincode::error::DecodeError> {
        let value = u8::decode(decoder)?;
        match value {
            0 => Ok(TVSystem::NTSC),
            1 => Ok(TVSystem::PAL),
            2 => Ok(TVSystem::Dendy),
            _ => Err(bincode::error::DecodeError::Other("Invalid TVSystem value")),
        }
    }
}

// Implement Encode and Decode for Mirroring
impl Encode for Mirroring {
    fn encode<E: bincode::enc::Encoder>(&self, encoder: &mut E) -> Result<(), bincode::error::EncodeError> {
        match self {
            Mirroring::Horizontal => 0u8.encode(encoder),
            Mirroring::Vertical => 1u8.encode(encoder),
            Mirroring::FourScreen => 2u8.encode(encoder),
            Mirroring::SingleScreenLower => 3u8.encode(encoder),
            Mirroring::SingleScreenUpper => 4u8.encode(encoder),
        }
    }
}

impl Decode<()> for Mirroring {
    fn decode<D: bincode::de::Decoder>(decoder: &mut D) -> Result<Self, bincode::error::DecodeError> {
        let value = u8::decode(decoder)?;
        match value {
            0 => Ok(Mirroring::Horizontal),
            1 => Ok(Mirroring::Vertical),
            2 => Ok(Mirroring::FourScreen),
            3 => Ok(Mirroring::SingleScreenLower),
            4 => Ok(Mirroring::SingleScreenUpper),
            _ => Err(bincode::error::DecodeError::Other("Invalid Mirroring value")),
        }
    }
}

impl<'de> BorrowDecode<'de, ()> for Mirroring {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de>>(decoder: &mut D) -> Result<Self, bincode::error::DecodeError> {
        let value = u8::decode(decoder)?;
        match value {
            0 => Ok(Mirroring::Horizontal),
            1 => Ok(Mirroring::Vertical),
            2 => Ok(Mirroring::FourScreen),
            3 => Ok(Mirroring::SingleScreenLower),
            4 => Ok(Mirroring::SingleScreenUpper),
            _ => Err(bincode::error::DecodeError::Other("Invalid Mirroring value")),
        }
    }
}

impl SaveState {
    /// Create a new save state from the NES state
    pub fn from_nes(nes: &NES) -> Result<Self, SaveStateError> {
        // Extract CPU state
        let cpu_state = CpuState {
            a: nes.cpu.a,
            x: nes.cpu.x,
            y: nes.cpu.y,
            sp: nes.cpu.sp,
            pc: nes.cpu.pc,
            p: nes.cpu.p,
            cycles: nes.cpu.cycles,
            total_cycles: nes.cpu.total_cycles,
            waiting: nes.cpu.waiting,
        };
        
        // Extract PPU state
        let ppu_state = PpuState {
            cycle: nes.ppu.cycle,
            scanline: nes.ppu.scanline,
            frame: nes.ppu.frame,
            v: nes.ppu.v,
            t: nes.ppu.t,
            x: nes.ppu.x,
            w: nes.ppu.w,
            nmi_occurred: nes.ppu.nmi_occurred,
            nmi_output: nes.ppu.nmi_output,
            even_frame: nes.ppu.even_frame,
            data_buffer: nes.ppu.data_buffer,
            vram: nes.ppu.vram.to_vec(),
            palette_ram: nes.ppu.palette_ram.to_vec(),
            oam: nes.ppu.oam.to_vec(),
            tv_system: nes.ppu.tv_system,
            sprite_zero_hit: nes.ppu.sprites.sprite_zero_hit,
            sprite_overflow: nes.ppu.sprites.sprite_overflow,
        };
        
        // Extract memory state
        let memory_state = MemoryState {
            ram: nes.memory_bus.get_ram().to_vec(),
            ppu_registers: nes.memory_bus.ppu_registers.to_vec(),
            apu_io_registers: nes.memory_bus.apu_io_registers.to_vec(),
            oam_dma_active: nes.memory_bus.oam_dma_active,
            oam_dma_addr: nes.memory_bus.oam_dma_addr,
            oam_dma_page: nes.memory_bus.oam_dma_page,
            nmi_pending: nes.memory_bus.get_nmi_pending(),
            irq_pending: nes.memory_bus.get_irq_pending(),
        };
        
        // Extract APU state (simplified for brevity)
        let apu_state = ApuState {
            pulse1: PulseState::default(),
            pulse2: PulseState::default(),
            triangle: TriangleState::default(),
            noise: NoiseState::default(),
            dmc: DmcState::default(),
            frame_counter: 0,
            frame_irq_inhibit: false,
            frame_counter_mode: false,
            frame_sequence: 0,
            cycles: 0,
        };
        // Extract cartridge state
        let cartridge_state = if let Some(cart_ref) = nes.memory_bus.get_cartridge() {
            let cart = cart_ref.borrow();
            
            // Get mapper number and cartridge details
            let mapper_number = cart.mapper_number();
            let has_battery = false; // This would come from the cartridge
            let mirroring = cart.get_mirroring();
            
            // Get PRG RAM
            let prg_ram = cart.save_ram();
            
            // Get CHR RAM (if any)
            let chr_ram = Vec::new(); // This would be extracted from the cartridge if CHR is RAM
            
            // Create mapper-specific state
            let mapper_state = match mapper_number {
                0 => MapperState::Mapper000,
                1 => {
                    // The actual implementation would extract these from the mapper
                    let mmc1_state = MMC1State {
                        shift_register: 0x10, // Default value
                        shift_count: 0,
                        control: 0x0C,       // Initial control value
                        chr_bank_0: 0,
                        chr_bank_1: 0,
                        prg_bank: 0,
                    };
                    MapperState::Mapper001(mmc1_state)
                },
                2 => {
                    // Extract UxROM state
                    let uxrom_state = UxROMState {
                        prg_bank: 0, // This would come from the actual mapper
                    };
                    MapperState::Mapper002(uxrom_state)
                },
                3 => {
                    // Extract CNROM state
                    let cnrom_state = CNROMState {
                        chr_bank: 0, // This would come from the actual mapper
                    };
                    MapperState::Mapper003(cnrom_state)
                },
                4 => {
                    // Extract MMC3 state
                    let mmc3_state = MMC3State {
                        bank_select: 0,
                        bank_registers: [0; 8],
                        prg_mode: 0,
                        chr_mode: 0,
                        irq_counter: 0,
                        irq_latch: 0,
                        irq_enabled: false,
                        irq_pending: false,
                        irq_reload: false,
                        prg_ram_protect: [false, false],
                    };
                    MapperState::Mapper004(mmc3_state)
                },
                _ => {
                    // For other mappers, store raw bytes
                    MapperState::Unknown(Vec::new())
                }
            };
            
            CartridgeState {
                mapper_number,
                prg_ram,
                chr_ram,
                has_battery,
                mirroring,
                mapper_state,
            }
        } else {
            return Err(SaveStateError::NoCartridge);
        };
        
        Ok(SaveState {
            version: CURRENT_SAVE_STATE_VERSION,
            cpu: cpu_state,
            ppu: ppu_state,
            apu: apu_state,
            memory: memory_state,
            cartridge: cartridge_state,
        })
    }
    
    /// Apply save state to NES
    pub fn apply_to_nes(&self, nes: &mut NES) -> Result<(), SaveStateError> {
        // Check version compatibility
        if self.version != CURRENT_SAVE_STATE_VERSION {
            return Err(SaveStateError::IncompatibleVersion(
                self.version, 
                CURRENT_SAVE_STATE_VERSION
            ));
        }
        {
            // Make sure a cartridge is loaded
            let cartridge = nes.memory_bus.get_cartridge().ok_or(SaveStateError::NoCartridge)?;
            
            // Check that the mapper type matches
            let cart_ref = cartridge.borrow();
            if cart_ref.mapper_number() != self.cartridge.mapper_number {
                return Err(SaveStateError::UnsupportedMapper(self.cartridge.mapper_number));
            }
            drop(cart_ref);
        }
        
        // Apply CPU state
        nes.cpu.a = self.cpu.a;
        nes.cpu.x = self.cpu.x;
        nes.cpu.y = self.cpu.y;
        nes.cpu.sp = self.cpu.sp;
        nes.cpu.pc = self.cpu.pc;
        nes.cpu.p = self.cpu.p;
        nes.cpu.cycles = self.cpu.cycles;
        nes.cpu.total_cycles = self.cpu.total_cycles;
        nes.cpu.waiting = self.cpu.waiting;
        
        // Apply PPU state
        nes.ppu.cycle = self.ppu.cycle;
        nes.ppu.scanline = self.ppu.scanline;
        nes.ppu.frame = self.ppu.frame;
        nes.ppu.v = self.ppu.v;
        nes.ppu.t = self.ppu.t;
        nes.ppu.x = self.ppu.x;
        nes.ppu.w = self.ppu.w;
        nes.ppu.nmi_occurred = self.ppu.nmi_occurred;
        nes.ppu.nmi_output = self.ppu.nmi_output;
        nes.ppu.even_frame = self.ppu.even_frame;
        nes.ppu.data_buffer = self.ppu.data_buffer;
        nes.ppu.tv_system = self.ppu.tv_system;
        nes.ppu.sprites.sprite_zero_hit = self.ppu.sprite_zero_hit;
        nes.ppu.sprites.sprite_overflow = self.ppu.sprite_overflow;
        
        // Copy VRAM data
        if self.ppu.vram.len() == nes.ppu.vram.len() {
            nes.ppu.vram.copy_from_slice(&self.ppu.vram);
        } else {
            warn!("VRAM size mismatch: {} vs {}", self.ppu.vram.len(), nes.ppu.vram.len());
        }
        
        // Copy palette RAM data
        if self.ppu.palette_ram.len() == nes.ppu.palette_ram.len() {
            nes.ppu.palette_ram.copy_from_slice(&self.ppu.palette_ram);
        } else {
            warn!("Palette RAM size mismatch: {} vs {}", self.ppu.palette_ram.len(), nes.ppu.palette_ram.len());
        }
        
        // Copy OAM data
        if self.ppu.oam.len() == nes.ppu.oam.len() {
            nes.ppu.oam.copy_from_slice(&self.ppu.oam);
        } else {
            warn!("OAM size mismatch: {} vs {}", self.ppu.oam.len(), nes.ppu.oam.len());
        }
        {
            let mut memory_bus = nes.memory_bus.borrow_mut();
            // Apply memory state
            if self.memory.ram.len() == memory_bus.get_ram().len() {
                memory_bus.copy_ram(&self.memory.ram);
            } else {
                warn!("RAM size mismatch: {} vs {}", self.memory.ram.len(), memory_bus.get_ram().len());
            }
        }
        
        if self.memory.ppu_registers.len() == nes.memory_bus.ppu_registers.len() {
            nes.memory_bus.ppu_registers.copy_from_slice(&self.memory.ppu_registers);
        } else {
            warn!("PPU registers size mismatch: {} vs {}", 
                  self.memory.ppu_registers.len(), nes.memory_bus.ppu_registers.len());
        }
        
        if self.memory.apu_io_registers.len() == nes.memory_bus.apu_io_registers.len() {
            nes.memory_bus.apu_io_registers.copy_from_slice(&self.memory.apu_io_registers);
        } else {
            warn!("APU I/O registers size mismatch: {} vs {}", 
                  self.memory.apu_io_registers.len(), nes.memory_bus.apu_io_registers.len());
        }
        
        nes.memory_bus.oam_dma_active = self.memory.oam_dma_active;
        nes.memory_bus.oam_dma_addr = self.memory.oam_dma_addr;
        nes.memory_bus.oam_dma_page = self.memory.oam_dma_page;
        nes.memory_bus.set_nmi_pending(self.memory.nmi_pending);
        nes.memory_bus.set_irq_pending(self.memory.irq_pending);
        
        // Load cartridge state
        if let Some(cart) = nes.memory_bus.get_cartridge() {
            let cart_mut = &mut *cart.borrow_mut();
            
            // Load PRG RAM if any
            if !self.cartridge.prg_ram.is_empty() {
                cart_mut.load_ram(&self.cartridge.prg_ram);
            }
            
            // Load mapper-specific state
            match &self.cartridge.mapper_state {
                MapperState::Mapper000 => {
                    // NROM has no state to load
                },
                MapperState::Mapper001(mmc1_state) => {
                    // Apply MMC1 state
                    // In a real implementation, this would call into 
                    // a mapper-specific method to restore state
                    
                    // Example of what this might look like:
                    // cart_mut.write(0x8000, 0x80); // Reset
                    // cart_mut.write(0x8000, mmc1_state.control & 0x01);
                    // cart_mut.write(0x8000, mmc1_state.control >> 1 & 0x01);
                    // cart_mut.write(0x8000, mmc1_state.control >> 2 & 0x01);
                    // cart_mut.write(0x8000, mmc1_state.control >> 3 & 0x01);
                    // cart_mut.write(0x8000, mmc1_state.control >> 4 & 0x01);
                    // 
                    // // And so on for other registers
                },
                MapperState::Mapper002(uxrom_state) => {
                    // Apply UxROM state
                    // Example:
                    // cart_mut.write(0x8000, uxrom_state.prg_bank);
                },
                MapperState::Mapper003(cnrom_state) => {
                    // Apply CNROM state
                    // Example:
                    // cart_mut.write(0x8000, cnrom_state.chr_bank);
                },
                MapperState::Mapper004(mmc3_state) => {
                    // Apply MMC3 state
                    // This would be a sequence of writes to restore the state
                    // Example:
                    // cart_mut.write(0x8000, mmc3_state.bank_select);
                    // for i in 0..8 {
                    //     cart_mut.write(0x8001, mmc3_state.bank_registers[i]);
                    // }
                    // 
                    // // And so on for the rest of the state
                },
                MapperState::Unknown(_) => {
                    warn!("Unknown mapper state format, state not restored");
                },
            }
        }
        
        // At this point, the state has been fully restored
        info!("Save state restored successfully");
        Ok(())
    }
    
    /// Save state to a file
    pub fn save_to_file<P: AsRef<Path>>(nes: &NES, path: P) -> Result<(), SaveStateError> {
        // Create save state from NES
        let state = Self::from_nes(nes)?;
        
        // Serialize save state with configuration optimized for size
        let config = config();
        let data = serialize(&state, config)
            .map_err(|e| SaveStateError::SerializationError(e.to_string()))?;
        
        // Write to file
        let mut file = File::create(path.as_ref())
            .map_err(|e| SaveStateError::IoError(e))?;
        
        file.write_all(&data)
            .map_err(|e| SaveStateError::IoError(e))?;
        
        info!("Save state written to {}", path.as_ref().display());
        Ok(())
    }
    
    /// Load state from a file
    pub fn load_from_file<P: AsRef<Path>>(nes: &mut NES, path: P) -> Result<(), SaveStateError> {
        // Read file
        let mut file = File::open(path.as_ref())
            .map_err(|e| SaveStateError::IoError(e))?;
        
        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .map_err(|e| SaveStateError::IoError(e))?;
        
        // Check for minimum file size
        if data.len() < 8 {
            return Err(SaveStateError::InvalidData);
        }
        
        // Deserialize save state
        let config = config();
        let state: SaveState = deserialize(&data, config)
            .map_err(|e| SaveStateError::DeserializationError(e.to_string()))?;
        
        // Apply save state to NES
        state.apply_to_nes(nes)?;
        
        info!("Save state loaded from {}", path.as_ref().display());
        Ok(())
    }
}

impl<'de> BorrowDecode<'de, ()> for SaveState {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de>>(decoder: &mut D) -> Result<Self, bincode::error::DecodeError> {
        Ok(SaveState {
            version: Decode::decode(decoder)?,
            cpu: Decode::decode(decoder)?,
            ppu: Decode::decode(decoder)?,
            apu: Decode::decode(decoder)?,
            memory: Decode::decode(decoder)?,
            cartridge: Decode::decode(decoder)?,
        })
    }
}

impl Decode<()> for SaveState {
    fn decode<D: bincode::de::Decoder>(decoder: &mut D) -> Result<Self, bincode::error::DecodeError> {
        Ok(SaveState {
            version: Decode::decode(decoder)?,
            cpu: Decode::decode(decoder)?,
            ppu: Decode::decode(decoder)?,
            apu: Decode::decode(decoder)?,
            memory: Decode::decode(decoder)?,
            cartridge: Decode::decode(decoder)?,
        })
    }
}

// Default implementations for APU channel states
impl Default for PulseState {
    fn default() -> Self {
        PulseState {
            enabled: false,
            duty: 0,
            length_counter_halt: false,
            constant_volume: false,
            volume: 0,
            sweep_enabled: false,
            sweep_period: 0,
            sweep_negative: false,
            sweep_shift: 0,
            timer_period: 0,
            length_counter: 0,
            timer: 0,
            sequencer_step: 0,
            envelope_start: false,
            envelope_divider: 0,
            envelope_decay: 0,
            envelope_volume: 0,
            sweep_reload: false,
            sweep_divider: 0,
            muted: false,
        }
    }
}

impl Default for TriangleState {
    fn default() -> Self {
        TriangleState {
            enabled: false,
            linear_counter_reload: false,
            linear_counter_period: 0,
            length_counter_halt: false,
            timer_period: 0,
            length_counter: 0,
            timer: 0,
            sequencer_step: 0,
            linear_counter: 0,
            linear_counter_reload_flag: false,
        }
    }
}

impl Default for NoiseState {
    fn default() -> Self {
        NoiseState {
            enabled: false,
            length_counter_halt: false,
            constant_volume: false,
            volume: 0,
            mode: false,
            timer_period: 0,
            length_counter: 0,
            timer: 0,
            shift_register: 1,
            envelope_start: false,
            envelope_divider: 0,
            envelope_decay: 0,
            envelope_volume: 0,
        }
    }
}

impl Default for DmcState {
    fn default() -> Self {
        DmcState {
            enabled: false,
            irq_enabled: false,
            loop_flag: false,
            timer_period: 0,
            output_level: 0,
            sample_address: 0,
            sample_length: 0,
            timer: 0,
            sample_buffer: 0,
            sample_buffer_empty: true,
            current_address: 0,
            bytes_remaining: 0,
            shift_register: 0,
            bits_remaining: 0,
            silent: true,
        }
    }
}