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

/// Encode a Wicki-Hayden coordinate relative to `center` as `(channel, note)`
/// for comma-bank fifths MIDI:
///
/// - `note` is the 12-TET MIDI note (`0..=127`, center = 60).
/// - `channel` (`0..=15`) is the signed 12-fifth comma bank modulo 16, with
///   channel 0 (MIDI Ch 1) carrying the central twelve fifths `D♭..F♯`
///   (`fifths ∈ -5..=+6`), Ch 2 carrying `+12` fifths (`+7..=+18`, from C♯),
///   and Ch 16 carrying `−12` fifths (`-17..=-6`, down from G♭).
///
/// Returns `None` rather than clamping or aliasing if the note lies outside
/// `0..=127` or the bank lies outside `-8..=+7`.
pub const fn encode_fifths_midi(coord: Coordinate, center: Coordinate) -> Option<(u8, u8)> {
    let dx = coord.x as i16 - center.x as i16;
    let dy = coord.y as i16 - center.y as i16;

    let note = 60 + 2 * dx - 5 * dy;
    if note < 0 || note > 127 {
        return None;
    }

    let fifths = 2 * dx + dy;
    let bank = (fifths + 5).div_euclid(12);
    if bank < -8 || bank > 7 {
        return None;
    }

    let channel = bank.rem_euclid(16) as u8;
    Some((channel, note as u8))
}

/// Inverse of [`encode_fifths_midi`]: recover the exact Wicki-Hayden
/// coordinate relative to `center` from `(channel, note)`.
pub const fn decode_fifths_midi(channel: u8, note: u8, center: Coordinate) -> Option<Coordinate> {
    if channel >= 16 || note >= 128 {
        return None;
    }

    let bank = if channel < 8 {
        channel as i16
    } else {
        channel as i16 - 16
    };
    let f0 = (7 * (note as i16) + 5).rem_euclid(12) - 5;
    let fifths = f0 + 12 * bank;

    let rel_key = note as i16 - 60;
    let diff = fifths - rel_key;
    if diff % 6 != 0 {
        return None;
    }
    let dy = diff / 6;
    if (fifths - dy) % 2 != 0 {
        return None;
    }
    let dx = (fifths - dy) / 2;

    let x = center.x as i16 + dx;
    let y = center.y as i16 + dy;
    if x < i8::MIN as i16 || x > i8::MAX as i16 || y < i8::MIN as i16 || y > i8::MAX as i16 {
        return None;
    }

    Some(Coordinate {
        x: x as i8,
        y: y as i8,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifths_midi_central_window_is_db_to_f_sharp_on_channel_zero() {
        let center = Coordinate { x: 1, y: 6 };
        // Center C4 (fifths = 0) -> Ch 0, MIDI 60
        assert_eq!(encode_fifths_midi(center, center), Some((0, 60)));
        // G4 (dx=1, dy=-1 -> fifths=+1, note=67) -> Ch 0, MIDI 67
        assert_eq!(
            encode_fifths_midi(Coordinate { x: 2, y: 5 }, center),
            Some((0, 67))
        );
        // F# (dx=3, dy=0 -> fifths=+6, note=66) -> Ch 0, MIDI 66
        assert_eq!(
            encode_fifths_midi(Coordinate { x: 4, y: 6 }, center),
            Some((0, 66))
        );
        // C# (dx=4, dy=-1 -> fifths=+7, note=73) -> Ch 1 (+12 fifths), MIDI 73
        assert_eq!(
            encode_fifths_midi(Coordinate { x: 5, y: 5 }, center),
            Some((1, 73))
        );
        // Db (dx=-2, dy=-1 -> fifths=-5, note=61) -> Ch 0, MIDI 61
        assert_eq!(
            encode_fifths_midi(Coordinate { x: -1, y: 5 }, center),
            Some((0, 61))
        );
        // Gb (dx=-3, dy=0 -> fifths=-6, note=54) -> Ch 15 (-12 fifths), MIDI 54
        assert_eq!(
            encode_fifths_midi(Coordinate { x: -2, y: 6 }, center),
            Some((15, 54))
        );
    }

    #[test]
    fn fifths_midi_round_trips_wide_wicki_hayden_grid() {
        let center = Coordinate { x: 1, y: 6 };
        // Covers well beyond 5x25 and 7x50 boards
        for dx in -25i8..=25 {
            for dy in -10i8..=10 {
                let coord = Coordinate {
                    x: center.x + dx,
                    y: center.y + dy,
                };
                if let Some((ch, note)) = encode_fifths_midi(coord, center) {
                    assert_eq!(decode_fifths_midi(ch, note, center), Some(coord));
                }
            }
        }
    }

    #[test]
    fn fifths_midi_drops_out_of_range_instead_of_clamping_or_aliasing() {
        let center = Coordinate { x: 0, y: 0 };
        // note = 60 + 2*0 - 5*13 = -5 -> must return None, not clamp to 0
        assert_eq!(encode_fifths_midi(Coordinate { x: 0, y: 13 }, center), None);
        // note = 60 + 2*0 - 5*(-14) = 130 -> must return None, not clamp to 127
        assert_eq!(
            encode_fifths_midi(Coordinate { x: 0, y: -14 }, center),
            None
        );
        // fifths = 2*46 + 0 = 92 -> bank = 8 (outside -8..=+7) -> must return None
        assert_eq!(encode_fifths_midi(Coordinate { x: 46, y: 0 }, center), None);
    }
}
