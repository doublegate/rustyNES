//! NES system implementation
//!
//! This module implements the core NES system, tying together the CPU, PPU, APU,
//! memory management, and input handling.

use anyhow::{Context, Result};
use log::{debug, info};
use sdl2::{
    event::Event,
    keyboard::Keycode,
    pixels::PixelFormatEnum,
    render::TextureCreator,
    video::WindowContext,
};

use crate::{
    apu::APU,
    cartridge::{Cartridge, ROMParseError},
    cpu::CPU,
    memory::MemoryBus,
    ppu::PPU,
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
    cpu: CPU,
    ppu: PPU,
    apu: APU,
    memory_bus: MemoryBus,
    controller1: Controller,
    controller2: Controller,
}

impl NES {
    /// Create a new NES system
    pub fn new() -> Self {
        let memory_bus = MemoryBus::new();
        let cpu = CPU::new();
        let ppu = PPU::new();
        let apu = APU::new();
        let controller1 = Controller::new();
        let controller2 = Controller::new();

        NES {
            cpu,
            ppu,
            apu,
            memory_bus,
            controller1,
            controller2,
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

    /// Reset the NES system to its initial state
    pub fn reset(&mut self) {
        self.cpu.reset();
        self.ppu.reset();
        self.apu.reset();
        self.memory_bus.reset();
    }

    /// Run the emulator
    pub fn run(&mut self) -> Result<()> {
        // Initialize SDL2
        let sdl_context = sdl2::init()
            .map_err(|e| anyhow::anyhow!("Failed to initialize SDL2: {}", e))?;
        
        let video_subsystem = sdl_context.video()
            .map_err(|e| anyhow::anyhow!("Failed to initialize SDL2 video subsystem: {}", e))?;
        let window = video_subsystem
            .window(
                "RustyNES",
                SCREEN_WIDTH * SCALE_FACTOR,
                SCREEN_HEIGHT * SCALE_FACTOR,
            )
            .position_centered()
            .build()
            .with_context(|| "Failed to create window")?;
        
        let mut canvas = window
            .into_canvas()
            .accelerated()
            .present_vsync()
            .build()
            .with_context(|| "Failed to create canvas")?;
        
        let texture_creator: TextureCreator<WindowContext> = canvas.texture_creator();
        let mut texture = texture_creator
            .create_texture_streaming(
                PixelFormatEnum::RGB24,
                SCREEN_WIDTH,
                SCREEN_HEIGHT,
            )
            .with_context(|| "Failed to create texture")?;
        canvas.set_scale(SCALE_FACTOR as f32, SCALE_FACTOR as f32)
            .map_err(|e| anyhow::anyhow!("Failed to set canvas scale: {}", e))?;
        
        let mut event_pump = sdl_context.event_pump()
            .map_err(|e| anyhow::anyhow!("Failed to get event pump: {}", e))?;

        // Main emulation loop
        'running: loop {
            // Handle events
            for event in event_pump.poll_iter() {
                match event {
                    Event::Quit { .. } | Event::KeyDown { keycode: Some(Keycode::Escape), .. } => {
                        break 'running;
                    },
                    Event::KeyDown { keycode: Some(key), .. } => {
                        self.handle_key_down(key);
                    },
                    Event::KeyUp { keycode: Some(key), .. } => {
                        self.handle_key_up(key);
                    },
                    _ => {}
                }
            }

            // Run one frame of emulation
            self.run_frame()?;
            
            // Update the screen
            texture.update(None, &self.ppu.get_frame_buffer(), SCREEN_WIDTH as usize * 3)
                .with_context(|| "Failed to update texture")?;
            
            canvas.copy(&texture, None, None)
                .map_err(|e| anyhow::anyhow!("Failed to copy texture to canvas: {}", e))?;
            
            canvas.present();
        }

        Ok(())
    }

    /// Run a single frame of emulation
    fn run_frame(&mut self) -> Result<()> {
        // A frame consists of a specific number of cycles
        // For NTSC NES: 29780 CPU cycles per frame (PPU runs at 3x CPU rate)
        const CYCLES_PER_FRAME: u32 = 29780;
        
        let mut cycles_run = 0;
        
        while cycles_run < CYCLES_PER_FRAME {
            // Run one CPU cycle
            let cpu_cycles = self.cpu.step(&mut self.memory_bus);
            cycles_run += cpu_cycles;
            
            // Run PPU cycles (3 PPU cycles per CPU cycle)
            for _ in 0..cpu_cycles * 3 {
                self.ppu.step(&mut self.memory_bus);
            }
            
            // Run APU cycles
            for _ in 0..cpu_cycles {
                self.apu.step(&mut self.memory_bus);
            }
        }
        
        debug!("Frame completed, {} cycles run", cycles_run);
        Ok(())
    }

    /// Handle key down events
    fn handle_key_down(&mut self, key: Keycode) {
        match key {
            Keycode::Z => self.controller1.set_button_pressed(Controller::BUTTON_A, true),
            Keycode::X => self.controller1.set_button_pressed(Controller::BUTTON_B, true),
            Keycode::Return => self.controller1.set_button_pressed(Controller::BUTTON_START, true),
            Keycode::RShift => self.controller1.set_button_pressed(Controller::BUTTON_SELECT, true),
            Keycode::Up => self.controller1.set_button_pressed(Controller::BUTTON_UP, true),
            Keycode::Down => self.controller1.set_button_pressed(Controller::BUTTON_DOWN, true),
            Keycode::Left => self.controller1.set_button_pressed(Controller::BUTTON_LEFT, true),
            Keycode::Right => self.controller1.set_button_pressed(Controller::BUTTON_RIGHT, true),
            _ => {}
        }
    }

    /// Handle key up events
    fn handle_key_up(&mut self, key: Keycode) {
        match key {
            Keycode::Z => self.controller1.set_button_pressed(Controller::BUTTON_A, false),
            Keycode::X => self.controller1.set_button_pressed(Controller::BUTTON_B, false),
            Keycode::Return => self.controller1.set_button_pressed(Controller::BUTTON_START, false),
            Keycode::RShift => self.controller1.set_button_pressed(Controller::BUTTON_SELECT, false),
            Keycode::Up => self.controller1.set_button_pressed(Controller::BUTTON_UP, false),
            Keycode::Down => self.controller1.set_button_pressed(Controller::BUTTON_DOWN, false),
            Keycode::Left => self.controller1.set_button_pressed(Controller::BUTTON_LEFT, false),
            Keycode::Right => self.controller1.set_button_pressed(Controller::BUTTON_RIGHT, false),
            _ => {}
        }
    }
}