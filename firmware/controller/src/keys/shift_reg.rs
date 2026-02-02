use embassy_executor::task;
use embassy_rp::gpio::{AnyPin, Input, Pull};
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
pub async fn keys_task_shift_reg(
    row_pins: [AnyPin; ROWS],
    // Shift Register Pins
    data_pin: AnyPin,  // GPIO 0
    latch_pin: AnyPin, // GPIO 1
    clock_pin: AnyPin, // GPIO 2
    sender: embassy_sync::channel::Sender<
        'static,
        embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
        crate::midi::MidiEvent,
        32,
    >,
) {
    use embassy_rp::gpio::{Level, Output};

    // Active High Configuration (Standard 74HC595 + Rows with Pull-Down)
    // Shift in '1', Rows read High when pressed.
    let rows: [Input<'static>; ROWS] = row_pins.map(|p| Input::new(p, Pull::Down));

    let mut data = Output::new(data_pin, Level::Low);
    let mut latch = Output::new(latch_pin, Level::Low);
    let mut clock = Output::new(clock_pin, Level::Low);

    info!("Keys task started. Shift Register Scanning (Active High).");

    let mut key_state = [[false; COLS]; ROWS];

    loop {
        // Ensure we start clean
        data.set_low();
        latch.set_low();
        clock.set_low();

        // ---------------------------------------------------------
        // Column 0: Shift in a High bit
        // ---------------------------------------------------------

        // 1. Set Data High
        data.set_high();

        // 2. Pulse Clock to shift '1' into Q0
        clock.set_high();
        Timer::after(Duration::from_micros(1)).await;
        clock.set_low();
        Timer::after(Duration::from_micros(1)).await;

        // 3. Pulse Latch to output
        latch.set_high();
        Timer::after(Duration::from_micros(1)).await;
        latch.set_low();
        Timer::after(Duration::from_micros(1)).await;

        scan_rows(0, &rows, &mut key_state, &sender).await;

        // ---------------------------------------------------------
        // Columns 1..COLS: Shift in Low bits (pushing the High bit along)
        // ---------------------------------------------------------
        data.set_low(); // We want 0s following the single 1

        for c_idx in 1..COLS {
            // Pulse Clock to shift
            clock.set_high();
            Timer::after(Duration::from_micros(1)).await;
            clock.set_low();
            Timer::after(Duration::from_micros(1)).await;

            // Pulse Latch to output
            latch.set_high();
            Timer::after(Duration::from_micros(1)).await;
            latch.set_low();
            Timer::after(Duration::from_micros(1)).await;

            scan_rows(c_idx, &rows, &mut key_state, &sender).await;
        }

        // Scan rate control: Fast as possible while yielding
        Timer::after(Duration::from_micros(100)).await;
    }
}

// Helper to scan rows and update state
async fn scan_rows(
    c_idx: usize,
    rows: &[Input<'static>; ROWS],
    key_state: &mut [[bool; COLS]; ROWS],
    sender: &embassy_sync::channel::Sender<
        'static,
        embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
        crate::midi::MidiEvent,
        32,
    >,
) {
    use crate::midi::ToU7;
    use log::error;

    for (r_idx, row) in rows.iter().enumerate() {
        let is_pressed = row.is_high();
        let was_pressed = key_state[r_idx][c_idx];

        if is_pressed != was_pressed {
            key_state[r_idx][c_idx] = is_pressed;

            // Debug: Raw Matrix Event (Optional, good for verification)
            if is_pressed {
                //info!("Raw Press: r{} c{}", r_idx, c_idx);
            }

            // State Changed
            // State Changed
            if let Some(coord) = CurrentLayout::key_to_coord(r_idx, c_idx) {
                // info!("Coord: {:?}", coord);

                if let Some(event) =
                    crate::tuning::get_midi_event::<CurrentLayout>(coord, 100.to_u7(), is_pressed)
                {
                    if let Err(_) = sender.try_send(event) {
                        error!("MIDI Channel Full! Dropping Event");
                    }

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
}
