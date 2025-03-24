//! Audio processing and output
//!
//! This module handles audio filtering, mixing, and output for the NES emulator.

mod dsp;
mod output;

pub use dsp::*;
pub use output::*;

use crate::apu::APU;

/// Audio sample format (16-bit signed PCM)
pub type Sample = i16;

/// Audio buffer (stereo interleaved samples)
pub type AudioBuffer = Vec<Sample>;

/// Audio system for processing and outputting sound
pub struct AudioSystem {
    /// Sample rate
    sample_rate: u32,
    
    /// Low-pass filter
    low_pass: LowPassFilter,
    
    /// High-pass filter
    high_pass: HighPassFilter,
    
    /// Audio output
    output: AudioOutput,
    
    /// Temporary buffer for processing
    buffer: AudioBuffer,
    
    /// Volume (0.0 - 1.0)
    volume: f32,
}

impl AudioSystem {
    /// Create a new audio system
    pub fn new(sample_rate: u32) -> Self {
        AudioSystem {
            sample_rate,
            low_pass: LowPassFilter::new(sample_rate, 12000.0),
            high_pass: HighPassFilter::new(sample_rate, 40.0),
            output: AudioOutput::new(sample_rate),
            buffer: Vec::new(),
            volume: 0.75,
        }
    }
    
    /// Process audio samples from the APU
    pub fn process(&mut self, apu: &mut APU) {
        // Get raw samples from APU
        let raw_samples = apu.get_samples();
        
        // Prepare buffer
        self.buffer.clear();
        self.buffer.reserve(raw_samples.len() * 2); // Stereo
        
        // Process samples
        for sample in raw_samples {
            // Apply volume
            let amplified = sample * self.volume;
            
            // Apply filters
            let filtered = self.high_pass.process(self.low_pass.process(amplified));
            
            // Convert to 16-bit PCM and duplicate for stereo
            let pcm = (filtered * 32767.0) as i16;
            self.buffer.push(pcm);  // Left
            self.buffer.push(pcm);  // Right
        }
        
        // Output audio
        self.output.queue_audio(&self.buffer);
    }
    
    /// Set volume (0.0 - 1.0)
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.max(0.0).min(1.0);
    }
    
    /// Get current volume
    pub fn volume(&self) -> f32 {
        self.volume
    }
    
    /// Pause audio output
    pub fn pause(&mut self) {
        self.output.pause();
    }
    
    /// Resume audio output
    pub fn resume(&mut self) {
        self.output.resume();
    }
    
    /// Close audio output
    pub fn close(&mut self) {
        self.output.close();
    }
}