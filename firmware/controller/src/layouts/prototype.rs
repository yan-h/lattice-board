use crate::layout::{Coordinate, Layout, LedIndex};

pub struct PrototypeLayout;
pub type CurrentLayout = PrototypeLayout;

// 1 = Key Present, 0 = No Key
#[rustfmt::skip]
static KEY_PRESENCE: [[u8; COLS]; ROWS] = [
    // Col 0  Col 1  Col 2  Col 3  Col 4  Col 5  Col 6
    [0,     1,     1,     0,     0,     0,     0], // Row 0
    [1,     1,     1,     1,     1,     0,     0], // Row 1
    [1,     1,     1,     1,     1,     1,     1], // Row 2
    [0,     0,     0,     1,     1,     1,     1], // Row 3
    [0,     0,     0,     0,     0,     1,     0], // Row 4
];
const NO_LED: u8 = 255;
pub const NUM_LEDS: usize = 19;

// LED Index Mapping
// 0, 1, 2... = LED Index, NO_LED = No LED
#[rustfmt::skip]
static LED_MATRIX: [[u8; COLS]; ROWS] = [
    // Col 0     Col 1     Col 2     Col 3     Col 4     Col 5     Col 6
    [NO_LED,   0,        1,        NO_LED,   NO_LED,   NO_LED,   NO_LED], // Row 0
    [2,        3,        4,        5,        6,        NO_LED,   NO_LED], // Row 1
    [7,        8,        9,        10,       11,       12,       13],     // Row 2
    [NO_LED,   NO_LED,   NO_LED,   14,       15,       16,       17],     // Row 3
    [NO_LED,   NO_LED,   NO_LED,   NO_LED,   NO_LED,   18,       NO_LED], // Row 4
];

impl Layout for PrototypeLayout {
    fn key_to_coord(row: usize, col: usize) -> Option<Coordinate> {
        if row < ROWS && col < COLS {
            if KEY_PRESENCE[row][col] == 1 {
                return Some(Coordinate {
                    x: col as i8,
                    y: row as i8,
                });
            }
        }
        None
    }

    fn center_coord() -> Coordinate {
        Coordinate { x: 3, y: 2 }
    }

    fn led_to_coord(idx: LedIndex) -> Option<Coordinate> {
        if idx < NUM_LEDS {
            Some(LED_LOOKUP[idx])
        } else {
            None
        }
    }

    fn coord_to_led(coord: Coordinate) -> Option<LedIndex> {
        let row = coord.y as usize;
        let col = coord.x as usize;

        if row >= ROWS || col >= COLS {
            return None;
        }

        let led = LED_MATRIX[row][col];
        if led != NO_LED {
            return Some(led as usize);
        }
        None
    }
}

// ---------------------------
// Static Lookup Generation
// ---------------------------

static LED_LOOKUP: [Coordinate; NUM_LEDS] =
    crate::layout::build_reversed_lookup::<ROWS, COLS, NUM_LEDS>(LED_MATRIX, NO_LED);

// Configuration Constants
pub const ROWS: usize = 5;
pub const COLS: usize = 7;

/// Helper macro to define the row pins.
/// Usage: `let rows = get_rows!(p);`
#[macro_export]
macro_rules! get_rows {
    ($p:ident) => {
        [
            $p.PIN_11.into(),
            $p.PIN_10.into(),
            $p.PIN_9.into(),
            $p.PIN_8.into(),
            $p.PIN_7.into(),
        ]
    };
}

/// Helper macro to define the column pins.
/// Usage: `let cols = get_cols!(p);`
#[macro_export]
macro_rules! get_cols {
    ($p:ident) => {
        [
            $p.PIN_28.into(),
            $p.PIN_27.into(),
            $p.PIN_26.into(),
            $p.PIN_15.into(),
            $p.PIN_14.into(),
            $p.PIN_13.into(),
            $p.PIN_12.into(),
        ]
    };
}
