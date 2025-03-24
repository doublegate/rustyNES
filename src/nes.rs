//! NES system implementation
//!
//! This module implements the core NES system, tying together the CPU, PPU, APU,
//! memory management, and input handling.

use anyhow::{Context, Result};
use log::{info, trace};
use sdl2::{
    event::Event,
    pixels::PixelFormatEnum,
    render::TextureCreator,
    video::WindowContext,
    keyboard::Keycode,
};
use std::path::Path;
use std::time::{Duration, Instant};
use std::rc::Rc;
use std::cell::RefCell;

use crate::{
    apu::APU,
    audio::AudioSystem,
    cartridge::{Cartridge, ROMParseError},
    cpu::CPU,
    memory::MemoryBus,
    ppu::{PPU, TVSystem},
    controller::Controller,
};

/// NES screen width in pixels
pub const SCREEN_WIDTH: u32 = 256;
/// NES screen height in pixels
pub const SCREEN_HEIGHT: u32 = 240;
/// Default scale factor for the display window
const SCALE_FACTOR: u32 = 3;

/// Represents the NES hardware system
pub struct NES {
    /// CPU
    pub cpu: CPU,
    
    /// PPU
    pub ppu: Rc<RefCell<PPU>>,
    
    /// APU
    pub apu: APU,
    
    /// Memory bus
    pub memory_bus: MemoryBus,
    
    /// Controller 1
    pub controller1: Controller,
    
    /// Controller 2
    pub controller2: Controller,
    
    /// Audio system
    pub audio_system: AudioSystem,
    
    /// Running state
    pub running: bool,
    
    /// Paused state
    pub paused: bool,
    
    /// Current frame count
    pub frame_count: u64,
    
    /// Frame timing
    pub frame_time: Duration,
    
    /// TV system (NTSC/PAL)
    pub tv_system: TVSystem,
    
    /// Last frame time
    last_frame_time: Instant,
    
    /// Frames per second
    pub fps: f64,

    /// Display scale factor
    pub scale_factor: u32,
}

impl NES {
    /// Create a new NES system
    pub fn new(tv_system: TVSystem, scale_factor: u32) -> Self {
        let ppu = Rc::new(RefCell::new(PPU::new(tv_system)));
        let memory_bus = MemoryBus::new(Rc::clone(&ppu));
        
        Self {
            cpu: CPU::new(),
            ppu: Rc::clone(&ppu),
            apu: APU::new(),
            memory_bus,
            controller1: Controller::new(),
            controller2: Controller::new(),
            audio_system: AudioSystem::new(44100), // Standard CD quality sample rate
            running: false,
            paused: false,
            frame_count: 0,
            frame_time: Duration::from_secs(0),
            tv_system,
            last_frame_time: Instant::now(),
            fps: 0.0,
            scale_factor,
        }
    }

    /// Load an NES cartridge from ROM data
    pub fn load_cartridge(&mut self, rom_data: &[u8]) -> Result<(), ROMParseError> {
        let cartridge = Cartridge::from_bytes(rom_data)?;
        self.memory_bus.insert_cartridge(cartridge);
        self.reset();
        
        info!("Cartridge loaded successfully");
        Ok(())
    }

    /// Load an NES cartridge from a file
    pub fn load_cartridge_from_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let rom_data = std::fs::read(&path)
            .with_context(|| format!("Failed to read ROM file: {}", path.as_ref().display()))?;
        
        self.load_cartridge(&rom_data)
            .with_context(|| format!("Failed to load ROM: {}", path.as_ref().display()))?;
        
        Ok(())
    }

    /// Reset the NES system to its initial state
    pub fn reset(&mut self) {
        self.cpu.reset();
        self.ppu.borrow_mut().reset();
        self.apu.reset();
        self.memory_bus.reset();
        self.controller1.reset();
        self.controller2.reset();
        self.frame_count = 0;
        self.running = false;
        self.paused = false;

        // Initialize PPU registers
        self.memory_bus.ppu_registers[0] = 0x00; // PPUCTRL - disable NMI initially
        self.memory_bus.ppu_registers[1] = 0x00; // PPUMASK - disable rendering initially
        
        // Run enough cycles to warm up the PPU (2 frames worth)
        let cycles_per_frame = match self.tv_system {
            TVSystem::NTSC => 29780,
            TVSystem::PAL => 33247,
            TVSystem::Dendy => 33247,
        };

        for _ in 0..2 {
            let mut cycles = cycles_per_frame;
            while cycles > 0 {
                let cpu_cycles = self.cpu.step(&mut self.memory_bus);
                cycles -= cpu_cycles;
                
                // PPU runs at 3x CPU rate
                for _ in 0..cpu_cycles * 3 {
                    self.ppu.borrow_mut().step(&mut self.memory_bus);
                }
            }
        }
        
        // Enable rendering
        self.memory_bus.ppu_registers[0] = 0x90; // PPUCTRL - enable NMI, background pattern table at 0x1000
        self.memory_bus.ppu_registers[1] = 0x1E; // PPUMASK - show background and sprites
    }

    /// Run the emulator
    // Update the run method to properly use scale_factor
    pub fn run(&mut self) -> Result<()> {
        // Initialize SDL2
        let sdl_context = sdl2::init()
            .map_err(|e| anyhow::anyhow!("Failed to initialize SDL2: {}", e))?;
        
        let video_subsystem = sdl_context.video()
            .map_err(|e| anyhow::anyhow!("Failed to initialize SDL2 video subsystem: {}", e))?;
        
        let window = video_subsystem
            .window(
                "RustyNES",
                SCREEN_WIDTH * self.scale_factor,
                SCREEN_HEIGHT * self.scale_factor,
            )
            .position_centered()
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create window: {}", e))?;
        
        let mut canvas = window
            .into_canvas()
            .accelerated()
            .present_vsync()
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create canvas: {}", e))?;
        
        canvas.set_scale(self.scale_factor as f32, self.scale_factor as f32)
            .map_err(|e| anyhow::anyhow!("Failed to set canvas scale: {}", e))?;
        
        let texture_creator: TextureCreator<WindowContext> = canvas.texture_creator();
        let mut texture = texture_creator
            .create_texture_streaming(
                PixelFormatEnum::RGB24,
                SCREEN_WIDTH,
                SCREEN_HEIGHT,
            )
            .with_context(|| "Failed to create texture")?;
        
        let mut event_pump = sdl_context.event_pump()
            .map_err(|e| anyhow::anyhow!("Failed to get event pump: {}", e))?;

        // Start the emulator
        self.running = true;
        
        // Frame timing
        let target_frame_time = match self.tv_system {
            TVSystem::NTSC => Duration::from_nanos(16_666_667), // 60Hz
            TVSystem::PAL => Duration::from_nanos(20_000_000),  // 50Hz
            TVSystem::Dendy => Duration::from_nanos(20_000_000), // 50Hz
        };
        
        info!("Emulation started with {} TV system", 
            match self.tv_system {
                TVSystem::NTSC => "NTSC",
                TVSystem::PAL => "PAL",
                TVSystem::Dendy => "Dendy",
            }
        );
        
        // Main emulation loop
        while self.running {
            // Handle events
            for event in event_pump.poll_iter() {
                match event {
                    Event::Quit { .. } => {
                        self.running = false;
                    },
                    Event::KeyDown { keycode: Some(keycode), .. } => {
                        self.handle_key_down(keycode);
                    },
                    Event::KeyUp { keycode: Some(keycode), .. } => {
                        self.handle_key_up(keycode);
                    },
                    _ => {}
                }
            }

            // Skip processing if paused
            if !self.paused {
                // Run one frame of emulation
                self.run_frame()?;
                
                // Update the screen texture
                texture.update(None, &self.ppu.borrow().get_frame_buffer(), SCREEN_WIDTH as usize * 3)
                    .with_context(|| "Failed to update texture")?;
                
                // Process audio
                self.audio_system.process(&mut self.apu);
                
                // Calculate FPS
                let now = Instant::now();
                let frame_duration = now.duration_since(self.last_frame_time);
                self.fps = 1.0 / frame_duration.as_secs_f64();
                self.last_frame_time = now;
                
                // Frame timing for steady frame rate
                if frame_duration < target_frame_time {
                    std::thread::sleep(target_frame_time - frame_duration);
                }
                
                self.frame_count += 1;
                
                // Print FPS every 60 frames
                if self.frame_count % 60 == 0 {
                    trace!("FPS: {:.2}", self.fps);
                }
            }
            
            // Render to screen
            canvas.clear();
            canvas.copy(&texture, None, None)
                .map_err(|e| anyhow::anyhow!("Failed to copy texture to canvas: {}", e))?;
            canvas.present();
        }
        
        // Cleanup audio
        self.audio_system.close();

        Ok(())
    }

    /// Run a single frame of emulation
    // Update the run_frame method to properly handle OAM DMA and timing
    pub fn run_frame(&mut self) -> Result<()> {
        // A frame consists of a specific number of cycles
        // For NTSC NES: 29780 CPU cycles per frame (PPU runs at 3x CPU rate)
        // For PAL NES: 33247 CPU cycles per frame
        let cycles_per_frame = match self.tv_system {
            TVSystem::NTSC => 29780,
            TVSystem::PAL => 33247,
            TVSystem::Dendy => 33247,
        };
        
        let mut cycles_remaining: i32 = cycles_per_frame;
        
        // Run CPU cycles until we've completed a frame
        while cycles_remaining > 0 {
            // Handle OAM DMA if active
            if self.memory_bus.oam_dma_active {
                let dma_cycles = 514; // DMA takes 514 cycles
                self.memory_bus.oam_dma_active = false;
                cycles_remaining = cycles_remaining.saturating_sub(dma_cycles as i32);
                continue;
            }
            
            // Run one CPU instruction
            let cpu_cycles = self.cpu.step(&mut self.memory_bus) as i32;
            cycles_remaining = cycles_remaining.saturating_sub(cpu_cycles);
            
            // Run PPU for 3x CPU cycles
            for _ in 0..(cpu_cycles * 3) {
                self.ppu.borrow_mut().step(&mut self.memory_bus);
            }
            
            // Run APU
            for _ in 0..cpu_cycles {
                self.apu.step(&mut self.memory_bus);
            }
        }
        
        Ok(())
    }

    /// Handle key down events
    fn handle_key_down(&mut self, keycode: Keycode) {
        match keycode {
            Keycode::Escape => self.running = false,
            Keycode::P => self.paused = !self.paused,
            Keycode::Z => self.controller1.set_button_pressed(Controller::BUTTON_A, true),      // A button
            Keycode::X => self.controller1.set_button_pressed(Controller::BUTTON_B, true),      // B button
            Keycode::Return => self.controller1.set_button_pressed(Controller::BUTTON_START, true),  // Start
            Keycode::RShift => self.controller1.set_button_pressed(Controller::BUTTON_SELECT, true), // Select
            Keycode::Left => self.controller1.set_button_pressed(Controller::BUTTON_LEFT, true),   // Left
            Keycode::Right => self.controller1.set_button_pressed(Controller::BUTTON_RIGHT, true),  // Right
            Keycode::Up => self.controller1.set_button_pressed(Controller::BUTTON_UP, true),     // Up
            Keycode::Down => self.controller1.set_button_pressed(Controller::BUTTON_DOWN, true),   // Down
            _ => {}
        }
    }

    /// Handle key up events
    fn handle_key_up(&mut self, keycode: Keycode) {
        match keycode {
            Keycode::Z => self.controller1.set_button_pressed(Controller::BUTTON_A, false),      // A button
            Keycode::X => self.controller1.set_button_pressed(Controller::BUTTON_B, false),      // B button
            Keycode::Return => self.controller1.set_button_pressed(Controller::BUTTON_START, false),  // Start
            Keycode::RShift => self.controller1.set_button_pressed(Controller::BUTTON_SELECT, false), // Select
            Keycode::Left => self.controller1.set_button_pressed(Controller::BUTTON_LEFT, false),   // Left
            Keycode::Right => self.controller1.set_button_pressed(Controller::BUTTON_RIGHT, false),  // Right
            Keycode::Up => self.controller1.set_button_pressed(Controller::BUTTON_UP, false),     // Up
            Keycode::Down => self.controller1.set_button_pressed(Controller::BUTTON_DOWN, false),   // Down
            _ => {}
        }
    }
}
