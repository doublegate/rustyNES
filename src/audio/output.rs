//! Audio output implementation
//!
//! This module handles outputting audio to the sound device.

use log::{debug, error, warn};
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::collections::VecDeque;

use super::Sample;

/// Audio callback for SDL2
struct NesAudioCallback {
    /// Audio buffer
    buffer: VecDeque<Sample>,
    
    /// Channel for receiving audio data
    receiver: Receiver<Vec<Sample>>,
}

impl AudioCallback for NesAudioCallback {
    type Channel = Sample;
    
    fn callback(&mut self, out: &mut [Self::Channel]) {
        // Check for new audio data
        while let Ok(samples) = self.receiver.try_recv() {
            for sample in samples {
                self.buffer.push_back(sample);
            }
        }
        
        // Fill output buffer
        for dst in out.iter_mut() {
            *dst = match self.buffer.pop_front() {
                Some(sample) => sample,
                None => 0,
            };
        }
    }
}

/// Audio output system
pub struct AudioOutput {
    /// SDL2 audio device
    device: Option<AudioDevice<NesAudioCallback>>,
    
    /// Sender for audio data
    sender: Sender<Vec<Sample>>,
    
    /// Sample rate
    sample_rate: u32,
}

impl AudioOutput {
    /// Create a new audio output
    // Add proper error handling for SDL initialization
    pub fn new(sample_rate: u32) -> Self {
        // Create channel for audio data
        let (sender, receiver) = channel();
        
        // Try to initialize SDL2 audio
        let device = match sdl2::init().and_then(|ctx| ctx.audio()) {
            Ok(audio_subsystem) => {
                // Configure audio
                let desired_spec = AudioSpecDesired {
                    freq: Some(sample_rate as i32),
                    channels: Some(2),  // Stereo
                    samples: Some(1024),
                };
                
                // Create audio device
                match audio_subsystem.open_playback(None, &desired_spec, |spec| {
                    debug!("Audio output initialized: {}Hz, {} channels, {} samples",
                          spec.freq, spec.channels, spec.samples);
                    
                    NesAudioCallback {
                        buffer: VecDeque::with_capacity(spec.samples as usize * 2),
                        receiver,
                    }
                }) {
                    Ok(device) => {
                        // Start audio playback
                        device.resume();
                        Some(device)
                    },
                    Err(err) => {
                        error!("Failed to open audio playback: {}", err);
                        None
                    }
                }
            },
            Err(err) => {
                error!("Failed to initialize SDL2 audio: {}", err);
                None
            }
        };
        
        AudioOutput {
            device,
            sender,
            sample_rate,
        }
    }
    
    /// Queue audio samples for playback
    pub fn queue_audio(&mut self, samples: &[Sample]) {
        if self.device.is_some() {
            if let Err(err) = self.sender.send(samples.to_vec()) {
                warn!("Failed to send audio data: {}", err);
            }
        }
    }
    
    /// Pause audio playback
    pub fn pause(&mut self) {
        if let Some(device) = &self.device {
            device.pause();
        }
    }
    
    /// Resume audio playback
    pub fn resume(&mut self) {
        if let Some(device) = &self.device {
            device.resume();
        }
    }
    
    /// Close audio device
    pub fn close(&mut self) {
        self.device = None;
    }
}