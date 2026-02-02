#![allow(dead_code)]

/// Represents a pitch class in microcents (1/1,000,000 of a cent).
/// Range: 0 to 1,199,999,999 (12 semitones * 100 cents * 1,000,000).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PitchClass(pub u32);

const MICRO_CENTS_PER_SEMITONE: u32 = 100_000_000;
const MICRO_CENTS_PER_OCTAVE: u32 = 12 * MICRO_CENTS_PER_SEMITONE;

impl PitchClass {
    /// Creates a new PitchClass from a floating point semitone value (0.0 - 11.999...).
    pub fn from_f32(val: f32) -> Self {
        let val = val % 12.0;
        let val = if val < 0.0 { val + 12.0 } else { val };
        // 1 semitone = 100,000,000 microcents
        let microcents = (val * MICRO_CENTS_PER_SEMITONE as f32) as u32;
        Self(microcents)
    }

    /// Creates a new PitchClass from raw microcents.
    /// Wraps automatically.
    pub fn new(microcents: u32) -> Self {
        Self(microcents % MICRO_CENTS_PER_OCTAVE)
    }

    /// Returns value in semitones (f32).
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / MICRO_CENTS_PER_SEMITONE as f32
    }
}

/// Represents an absolute pitch with an octave and a pitch class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pitch {
    pub pitch_class: PitchClass,
    pub octave: i32,
}

impl Pitch {
    pub fn new(pitch_class: PitchClass, octave: i32) -> Self {
        Self {
            pitch_class,
            octave,
        }
    }

    /// Converts a MIDI note number (0-127) to a Pitch.
    pub fn from_midi(note: u8) -> Self {
        let n = note as i32;
        let octave = (n / 12) - 1;
        let pc_val = (n % 12) as u32;

        let microcents = pc_val * MICRO_CENTS_PER_SEMITONE;

        Self {
            pitch_class: PitchClass::new(microcents),
            octave,
        }
    }

    /// Converts the pitch to a continuous absolute value (like fractional MIDI note).
    /// e.g. C4 = 60.0
    pub fn to_f32(&self) -> f32 {
        let octave_base = (self.octave + 1) as f32 * 12.0;
        octave_base + self.pitch_class.to_f32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pitch_class_normalization() {
        // Basic range
        assert_eq!(PitchClass::from_f32(0.0).0, 0);

        // 6.0 semitones
        assert_eq!(PitchClass::from_f32(6.0).0, 6 * 100_000_000);

        // Wrapping
        assert_eq!(PitchClass::from_f32(12.0).0, 0);

        // Negative
        // -1.0 semitones -> 11.0 semitones
        assert_eq!(PitchClass::from_f32(-1.0).0, 11 * 100_000_000);
    }

    #[test]
    fn test_pitch_midi_conversion() {
        // C4 = 60
        // Octave = (60 / 12) - 1 = 4
        // PC = 0
        let p = Pitch::from_midi(60);
        assert_eq!(p.octave, 4);
        assert_eq!(p.pitch_class.0, 0);

        // C#4 = 61
        // Octave 4, PC 1
        let p = Pitch::from_midi(61);
        assert_eq!(p.octave, 4);
        assert_eq!(p.pitch_class.0, 100_000_000);
    }
}
