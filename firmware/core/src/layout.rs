use core::fmt::Debug;

/// X and Y coordinates on the square grid.
/// On the controller, the grid is physically rotated by ~21 degrees, and slightly staggered.
///
/// Going one step to the right (x + 1) is a major second (2 fifths, down an octave)
/// Going one step up (y + 1) is an ascending perfect fourth (-1 fifth, up an octave)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coordinate {
    pub x: i8,
    pub y: i8,
}

/// Logical index of an LED on the strip.
pub type LedIndex = usize;

/// The interface that every board variant must implement.
///
/// This trait decouples the physical hardware (Matix Rows/Cols, LED Index)
/// from the logical musical representation (Notes).
pub trait Layout: Sync {
    /// Convert physical matrix coordinates to a logical lattice coordinate.
    fn key_to_coord(row: usize, col: usize) -> Option<Coordinate>;

    /// Convert a physical LED index to a logical lattice coordinate.
    fn led_to_coord(idx: LedIndex) -> Option<Coordinate>;

    /// Convert a logical lattice coordinate to a physical LED index.
    #[allow(dead_code)]
    fn coord_to_led(coord: Coordinate) -> Option<LedIndex>;

    /// Returns the logical Coordinate that corresponds to Middle C (MIDI 60).
    fn center_coord() -> Coordinate;

    /// Convert a Coordinate to a generic MIDI pitch (0-127).
    /// Default implementation maps `center_coord()` to 60.
    fn coord_to_midi(coord: Coordinate) -> u8 {
        let center = Self::center_coord();
        let base_note = 60i16; // Middle C

        // Calculate relative steps from center
        let dx = coord.x as i16 - center.x as i16;
        let dy = coord.y as i16 - center.y as i16;

        // Note = Base + (dx * 2) - (dy * 5)
        let note = base_note + (dx * 2) - (dy * 5);

        // Clamp to valid MIDI range
        if note < 0 {
            0
        } else if note > 127 {
            127
        } else {
            note as u8
        }
    }
}

/// Helper to generate a reverse lookup table from a matrix at compile time.
pub const fn build_reversed_lookup<const ROWS: usize, const COLS: usize, const NUM_LEDS: usize>(
    matrix: [[u8; COLS]; ROWS],
    no_led: u8,
) -> [Coordinate; NUM_LEDS] {
    let mut lookup = [Coordinate { x: 0, y: 0 }; NUM_LEDS];
    let mut r = 0;
    while r < ROWS {
        let mut c = 0;
        while c < COLS {
            let led_idx = matrix[r][c];
            if led_idx != no_led {
                let idx = led_idx as usize;
                if idx < NUM_LEDS {
                    lookup[idx] = Coordinate {
                        x: c as i8,
                        y: r as i8,
                    };
                }
            }
            c += 1;
        }
        r += 1;
    }
    lookup
}
