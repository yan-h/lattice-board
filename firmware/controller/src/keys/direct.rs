use embassy_executor::task;
use embassy_rp::gpio::{AnyPin, Input, Level, Output, Pull};
use embassy_time::{Duration, Timer};
use log::info;

use crate::layout::Layout;
use crate::layouts::{CurrentLayout, COLS, ROWS};
use core::cell::RefCell;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use heapless::Vec;
use lattice_board_core::layout::Coordinate;

// Shared state for Active Keys (Coordinates)
pub static ACTIVE_KEYS: Mutex<CriticalSectionRawMutex, RefCell<Vec<Coordinate, 16>>> =
    Mutex::new(RefCell::new(Vec::new()));

#[task]
pub async fn keys_task_direct(
    row_pins: [AnyPin; ROWS],
    col_pins: [AnyPin; COLS],
    sender: embassy_sync::channel::Sender<
        'static,
        embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
        crate::midi::MidiEvent,
        32,
    >,
) {
    use crate::midi::ToU7;

    // Direct GPIO Scanning
    // Columns are Outputs, Rows are Inputs.
    // Active High: Col set High, Row read High (Pull-Down).
    let rows: [Input<'static>; ROWS] = row_pins.map(|p| Input::new(p, Pull::Down));
    let mut cols: [Output<'static>; COLS] = col_pins.map(|p| Output::new(p, Level::Low));

    info!("Keys task started. Direct GPIO Scanning.");

    let mut key_state = [[false; COLS]; ROWS];

    loop {
        for (c_idx, col) in cols.iter_mut().enumerate() {
            // Activate Column
            col.set_high();
            // Allow signal to settle
            Timer::after(Duration::from_micros(10)).await;

            // Scan Rows
            for (r_idx, row) in rows.iter().enumerate() {
                let is_pressed = row.is_high();
                let was_pressed = key_state[r_idx][c_idx];

                if is_pressed != was_pressed {
                    key_state[r_idx][c_idx] = is_pressed;

                    if let Some(coord) = CurrentLayout::key_to_coord(r_idx, c_idx) {
                        // Use tuning module to generate event (Standard or Fifths)
                        if let Some(event) = crate::tuning::get_midi_event::<CurrentLayout>(
                            coord,
                            100.to_u7(),
                            is_pressed,
                        ) {
                            sender.send(event).await;

                            // Track Active keys
                            ACTIVE_KEYS.lock(|c| {
                                let mut keys = c.borrow_mut();
                                if is_pressed {
                                    if !keys.contains(&coord) {
                                        let _ = keys.push(coord);
                                    }
                                } else {
                                    keys.retain(|&x| x != coord);
                                }
                            });
                        }
                    }
                }
            }

            // Deactivate Column
            col.set_low();
        }

        Timer::after(Duration::from_millis(1)).await;
    }
}
