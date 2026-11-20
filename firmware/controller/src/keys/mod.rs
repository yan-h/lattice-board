#[cfg(feature = "layout-5x25")]
pub mod shift_reg;
#[cfg(feature = "layout-5x25")]
pub use shift_reg::*;

#[cfg(feature = "layout-prototype")]
pub mod direct;
#[cfg(feature = "layout-prototype")]
pub use direct::*;

use crate::layouts::{CurrentLayout, COLS, ROWS};
use crate::midi::{NoteEvent, NoteSender};
use core::cell::RefCell;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_time::{Duration, Instant};
use heapless::Vec;
use lattice_board_core::debounce::Debouncer;
use lattice_board_core::layout::{encode_fifths_midi, Coordinate, Layout};
use wmidi::{Channel, Note};

/// Held keys, for the LEDs and the dashboard.
pub static ACTIVE_KEYS: Mutex<CriticalSectionRawMutex, RefCell<Vec<Coordinate, 16>>> =
    Mutex::new(RefCell::new(Vec::new()));

/// How long a key ignores its switch after a press or release counts.
const DEBOUNCE: Duration = Duration::from_millis(5);

/// The MIDI channel and note a key sends: its 12-TET note, on the channel of
/// its 12-fifth comma bank.
pub fn key_note(coord: Coordinate) -> Option<(Channel, Note)> {
    let (channel, note) = encode_fifths_midi(coord, CurrentLayout::center_coord())?;
    Some((
        Channel::from_index(channel).ok()?,
        Note::try_from(note).ok()?,
    ))
}

/// Turns the scanners' raw switch readings into debounced key events.
pub struct KeyEvents {
    debouncer: Debouncer<{ ROWS * COLS }>,
    sender: NoteSender,
}

impl KeyEvents {
    pub fn new(sender: NoteSender) -> Self {
        Self {
            debouncer: Debouncer::new(),
            sender,
        }
    }

    /// Feeds the switch at `(row, col)`'s reading, taken at `now`.
    pub async fn reading(&mut self, row: usize, col: usize, pressed: bool, now: Instant) {
        let Some(pressed) = self.debouncer.update(
            row * COLS + col,
            pressed,
            now.as_ticks(),
            DEBOUNCE.as_ticks(),
        ) else {
            return;
        };
        let Some(coord) = CurrentLayout::key_to_coord(row, col) else {
            return;
        };

        ACTIVE_KEYS.lock(|keys| {
            let mut keys = keys.borrow_mut();
            if pressed {
                if !keys.contains(&coord) {
                    let _ = keys.push(coord);
                }
            } else {
                keys.retain(|&k| k != coord);
            }
        });

        if let Some((channel, note)) = key_note(coord) {
            self.sender
                .send(NoteEvent {
                    channel,
                    note,
                    on: pressed,
                })
                .await;
        }
    }
}
