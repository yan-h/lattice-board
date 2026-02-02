use crate::layout::{Coordinate, Layout, LedIndex};

pub struct Layout5x25;
pub type CurrentLayout = Layout5x25;

// Configuration Constants
pub const ROWS: usize = 10;
pub const COLS: usize = 13;
pub const NUM_LEDS: usize = 123; // Two are missing to make room for MCU

const NO_LED: u8 = 255;

// Need to convert PCB rows/cols to logical rows/cols.
// Each PCB row forms a zigzag pattern in blocks of 6. See example.
//
// PCB row 0: abcdef ghijkl ... (12 cols total. a is PCB col 1)
// PCB row 1: ABCD EFGHIJ KLM ... (13 cols total. A is PCB col 0)
// Pattern repeats on row 2 and below. Note the staggered nature of row 1.
//
//   Col  0   1   2   3   4   5   6   7   8   9
//
// Row 0  A  <a - b - c
//        |           |
//     1  B - C - D>  d - e - f>
//
//     2             <E - F - G  <g - h - i
//                            |           |
//     3                      H - I - J>  j - k - l>
//
//     4                                 <K - L - M ...

static X_PATTERN: [usize; 6] = [0, 1, 2, 2, 3, 4];
static Y_PATTERN: [usize; 6] = [0, 0, 0, 1, 1, 1];

// Offset between each successive block of 6
static BLOCK_OFFSET_COLS: usize = 5;
static BLOCK_OFFSET_ROWS: usize = 2;

/// Calculates the logical coordinate for a given physical (row, col).
/// This is called at compile time to populate the lookup table.
const fn calculate_coordinate(row: usize, col: usize) -> Option<Coordinate> {
    // Specific Missing LEDs
    if (row == 1 && col == 0) || (row == 0 && col == 1) {
        return None;
    }

    if row % 2 == 0 {
        if col == 0 || col >= 13 {
            return None;
        }
        let block_count = (col - 1) / 6;
        let block_idx = (col - 1) % 6;

        let x_pat = X_PATTERN[block_idx] as i8;
        let y_pat = Y_PATTERN[block_idx] as i8;

        let block_offset_x = (BLOCK_OFFSET_COLS * block_count) as i8;
        let block_offset_y = (BLOCK_OFFSET_ROWS * block_count) as i8;

        Some(Coordinate {
            x: x_pat + block_offset_x - (row / 2) as i8,
            y: (row as i8) + y_pat + block_offset_y,
        })
    } else {
        if col >= 13 {
            return None;
        }

        let block_count = (col + 2) / 6;
        let block_idx = (col + 2) % 6;

        let x_pat = X_PATTERN[block_idx] as i8;
        let y_pat = Y_PATTERN[block_idx] as i8;

        let block_offset_x = (BLOCK_OFFSET_COLS * block_count) as i8;
        let block_offset_y = (BLOCK_OFFSET_ROWS * block_count) as i8;

        Some(Coordinate {
            x: -2 + x_pat + block_offset_x - ((row + 1) / 2) as i8,
            y: (row as i8) - 1 + y_pat + block_offset_y,
        })
    }
}

// Generate the lookup table at compile time
const fn build_key_map() -> [[Option<Coordinate>; COLS]; ROWS] {
    let mut map = [[None; COLS]; ROWS];
    let mut r = 0;
    while r < ROWS {
        let mut c = 0;
        while c < COLS {
            map[r][c] = calculate_coordinate(r, c);
            c += 1;
        }
        r += 1;
    }
    map
}

// The actual lookup table used at runtime.
// Because it is `static` and initialized by a `const fn`, it is computed at compile time!
static KEY_MAP: [[Option<Coordinate>; COLS]; ROWS] = build_key_map();

// ----------------------------------------------------------------------------
// LED Mapping Logic (Boilerplate)
// ----------------------------------------------------------------------------

/// Calculates the LED index (0-122) for a given physical (row, col).
/// Returns NO_LED (255) if no LED is present at that position.
const fn calculate_led_index(row: usize, col: usize) -> u8 {
    // Bounds check
    if row >= ROWS || col >= COLS {
        return NO_LED;
    }

    // Specific Missing LEDs
    if (row == 1 && col == 0) || (row == 0 && col == 1) {
        return NO_LED;
    }

    // Calculate "Raw Index" based on physical snake pattern
    // This maps the 2D grid to a 1D sequence of "Potential LED Positions"
    let raw_idx = if row % 2 == 0 {
        // Even rows: left to right
        if col == 0 {
            // Even rows start at col 1
            return NO_LED;
        }
        (25 * (row / 2)) + col - 1
    } else {
        // Odd rows: right to left
        let base = ((row / 2) + 1) * 25 - 1;
        base - col
    };

    // Shift index to account for missing LEDs
    if raw_idx >= 25 {
        // Shift by -2 (skipping 2 gaps)
        (raw_idx - 2) as u8
    } else if raw_idx >= 1 {
        // Shift by -1 (skipping 1 gap)
        (raw_idx - 1) as u8
    } else {
        // raw_idx 0 (Gap 1)
        NO_LED
    }
}

// Generate the LED matrix (Physical (r,c) -> LED Index) at compile time
const fn build_led_matrix() -> [[u8; COLS]; ROWS] {
    let mut map = [[NO_LED; COLS]; ROWS];
    let mut r = 0;
    while r < ROWS {
        let mut c = 0;
        while c < COLS {
            // Direct physical mapping
            map[r][c] = calculate_led_index(r, c);
            c += 1;
        }
        r += 1;
    }
    map
}

// LED Index Mapping
static LED_MATRIX: [[u8; COLS]; ROWS] = build_led_matrix();

impl Layout for Layout5x25 {
    fn key_to_coord(row: usize, col: usize) -> Option<Coordinate> {
        if row < ROWS && col < COLS {
            return KEY_MAP[row][col];
        }
        None
    }

    fn center_coord() -> Coordinate {
        Coordinate { x: 1, y: 6 }
    }

    fn led_to_coord(idx: LedIndex) -> Option<Coordinate> {
        if idx < NUM_LEDS {
            Some(LED_LOOKUP[idx])
        } else {
            None
        }
    }

    fn coord_to_led(coord: Coordinate) -> Option<LedIndex> {
        // Linear search because we don't have a Coordinate->Index map,
        // and using LED_MATRIX[y][x] is wrong (x/y != r/c).
        // 123 items is fast enough.
        let mut i = 0;
        while i < NUM_LEDS {
            if LED_LOOKUP[i].x == coord.x && LED_LOOKUP[i].y == coord.y {
                return Some(i);
            }
            i += 1;
        }
        None
    }
}

// ---------------------------
// Static Lookup Generation
// ---------------------------

const fn build_led_lookup() -> [Coordinate; NUM_LEDS] {
    let mut lookup = [Coordinate { x: 0, y: 0 }; NUM_LEDS];
    let mut r = 0;
    while r < ROWS {
        let mut c = 0;
        while c < COLS {
            let led_idx = LED_MATRIX[r][c];
            if led_idx != NO_LED {
                let idx = led_idx as usize;
                if idx < NUM_LEDS {
                    if let Some(coord) = calculate_coordinate(r, c) {
                        lookup[idx] = coord;
                    }
                }
            }
            c += 1;
        }
        r += 1;
    }
    lookup
}

static LED_LOOKUP: [Coordinate; NUM_LEDS] = build_led_lookup();

/// Helper macro to define the row pins.
/// Usage: `let rows = get_rows!(p);`
/// Returns the available pins in 10-29 range on RP2040-Zero: 10,11,12,13,14,15, 26,27,28,29
#[macro_export]
macro_rules! get_rows {
    ($p:ident) => {
        [
            $p.PIN_10.into(),
            $p.PIN_11.into(),
            $p.PIN_12.into(),
            $p.PIN_13.into(),
            $p.PIN_14.into(),
            $p.PIN_15.into(),
            $p.PIN_26.into(),
            $p.PIN_27.into(),
            $p.PIN_28.into(),
            $p.PIN_29.into(),
        ]
    };
}

/// Debug function to print the current key map
#[allow(dead_code)]
pub fn log_key_map() {
    log::info!("--- Key Map Start ---");
    for (r, row) in KEY_MAP.iter().enumerate() {
        for (c, coord) in row.iter().enumerate() {
            if let Some(coord) = coord {
                log::info!("R{} C{}: ({}, {})", r, c, coord.x, coord.y);
            }
        }
    }
    log::info!("--- Key Map End ---");
}

/// Debug function to print the current LED map
#[allow(dead_code)]
pub fn log_led_map() {
    log::info!("--- LED Map Start ---");
    for (r, row) in LED_MATRIX.iter().enumerate() {
        for (c, &led_idx) in row.iter().enumerate() {
            if led_idx != NO_LED {
                log::info!("LED {} at R{} C{}", led_idx, r, c);
            }
        }
    }
    log::info!("--- LED Map End ---");
}
