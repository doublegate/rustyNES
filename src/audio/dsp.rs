//! Digital Signal Processing for audio
//!
//! This module provides DSP filters for audio processing.

/// Low-pass filter (attenuate high frequencies)
pub struct LowPassFilter {
    /// Sample rate
    sample_rate: f32,
    
    /// Cutoff frequency
    cutoff: f32,
    
    /// Filter coefficient
    alpha: f32,
    
    /// Previous output
    prev_output: f32,
}

/// High-pass filter (attenuate low frequencies)
pub struct HighPassFilter {
    /// Sample rate
    sample_rate: f32,
    
    /// Cutoff frequency
    cutoff: f32,
    
    /// Filter coefficient
    alpha: f32,
    
    /// Previous input
    prev_input: f32,
    
    /// Previous output
    prev_output: f32,
}

impl LowPassFilter {
    /// Create a new low-pass filter
    pub fn new(sample_rate: u32, cutoff: f32) -> Self {
        let sample_rate = sample_rate as f32;
        let dt = 1.0 / sample_rate;
        let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff);
        let alpha = dt / (dt + rc);
        
        LowPassFilter {
            sample_rate,
            cutoff,
            alpha,
            prev_output: 0.0,
        }
    }
    
    /// Process a sample through the filter
    pub fn process(&mut self, input: f32) -> f32 {
        self.prev_output = self.prev_output + self.alpha * (input - self.prev_output);
        self.prev_output
    }
    
    /// Set the cutoff frequency
    pub fn set_cutoff(&mut self, cutoff: f32) {
        self.cutoff = cutoff;
        let dt = 1.0 / self.sample_rate;
        let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff);
        self.alpha = dt / (dt + rc);
    }
}

impl HighPassFilter {
    /// Create a new high-pass filter
    pub fn new(sample_rate: u32, cutoff: f32) -> Self {
        let sample_rate = sample_rate as f32;
        let dt = 1.0 / sample_rate;
        let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff);
        let alpha = rc / (dt + rc);
        
        HighPassFilter {
            sample_rate,
            cutoff,
            alpha,
            prev_input: 0.0,
            prev_output: 0.0,
        }
    }
    
    /// Process a sample through the filter
    pub fn process(&mut self, input: f32) -> f32 {
        self.prev_output = self.alpha * (self.prev_output + input - self.prev_input);
        self.prev_input = input;
        self.prev_output
    }
    
    /// Set the cutoff frequency
    pub fn set_cutoff(&mut self, cutoff: f32) {
        self.cutoff = cutoff;
        let dt = 1.0 / self.sample_rate;
        let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff);
        self.alpha = rc / (dt + rc);
    }
}