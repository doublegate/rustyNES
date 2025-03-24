//! Controller implementation
//!
//! The NES has two controller ports, each supporting the standard NES controller.
//! This module handles the state of the controllers and the reading/writing of
//! controller data.

/// NES Controller
pub struct Controller {
    /// Current button state (8 buttons)
    button_state: u8,
    
    /// Shift register for serial reading
    shift_register: u8,
    
    /// Strobe flag
    strobe: bool,
}

impl Controller {
    /// Button bitmasks
    pub const BUTTON_A: u8 = 0x01;
    pub const BUTTON_B: u8 = 0x02;
    pub const BUTTON_SELECT: u8 = 0x04;
    pub const BUTTON_START: u8 = 0x08;
    pub const BUTTON_UP: u8 = 0x10;
    pub const BUTTON_DOWN: u8 = 0x20;
    pub const BUTTON_LEFT: u8 = 0x40;
    pub const BUTTON_RIGHT: u8 = 0x80;

    /// Create a new controller
    pub fn new() -> Self {
        Controller {
            button_state: 0,
            shift_register: 0,
            strobe: false,
        }
    }

    /// Reset the controller
    pub fn reset(&mut self) {
        self.button_state = 0;
        self.shift_register = 0;
        self.strobe = false;
    }

    /// Set a button state
    pub fn set_button_pressed(&mut self, button: u8, pressed: bool) {
        if pressed {
            self.button_state |= button;
        } else {
            self.button_state &= !button;
        }
        
        // When strobe is high, continuously reload shift register
        if self.strobe {
            self.shift_register = self.button_state;
        }
    }

    /// Write to the controller (strobe)
    pub fn write(&mut self, value: u8) {
        let new_strobe = (value & 0x01) != 0;
        
        // If strobe goes high, reload shift register
        if new_strobe && !self.strobe {
            self.shift_register = self.button_state;
        }
        
        self.strobe = new_strobe;
    }

    /// Read from the controller
    pub fn read(&mut self) -> u8 {
        if self.strobe {
            // When strobe is high, continuously return button A state (bit 0)
            return self.button_state & 0x01;
        }
        
        // When strobe is low, shift out bits and return lowest bit
        let result = self.shift_register & 0x01;
        self.shift_register = (self.shift_register >> 1) | 0x80; // Shift and set high bit
        result
    }

    /// Get the strobe state
    pub fn get_strobe(&self) -> bool {
        self.strobe
    }

    /// Set the strobe state
    pub fn set_strobe(&mut self, value: bool) {
        let old_strobe = self.strobe;
        self.strobe = value;
        
        // If strobe goes high, reload shift register
        if value && !old_strobe {
            self.shift_register = self.button_state;
        }
    }
}