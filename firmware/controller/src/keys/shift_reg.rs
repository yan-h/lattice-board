use embassy_executor::task;
use embassy_rp::gpio::{AnyPin, Input, Pull};
use embassy_time::{Duration, Instant, Timer};
use log::info;

use super::KeyEvents;
use crate::layouts::{COLS, ROWS};
use crate::midi::NoteSender;

#[task]
pub async fn keys_task_shift_reg(
    row_pins: [AnyPin; ROWS],
    // Shift Register Pins
    data_pin: AnyPin,  // GPIO 0
    latch_pin: AnyPin, // GPIO 1
    clock_pin: AnyPin, // GPIO 2
    sender: NoteSender,
) {
    use embassy_rp::gpio::{Level, Output};

    // Active High Configuration (Standard 74HC595 + Rows with Pull-Down)
    // Shift in '1', Rows read High when pressed.
    let rows: [Input<'static>; ROWS] = row_pins.map(|p| Input::new(p, Pull::Down));

    let mut data = Output::new(data_pin, Level::Low);
    let mut latch = Output::new(latch_pin, Level::Low);
    let mut clock = Output::new(clock_pin, Level::Low);

    info!("Keys task started. Shift Register Scanning (Active High).");

    let mut events = KeyEvents::new(sender);

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

        scan_rows(0, &rows, &mut events).await;

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

            scan_rows(c_idx, &rows, &mut events).await;
        }

        // Scan rate control: Fast as possible while yielding
        Timer::after(Duration::from_micros(100)).await;
    }
}

async fn scan_rows(c_idx: usize, rows: &[Input<'static>; ROWS], events: &mut KeyEvents) {
    let now = Instant::now();
    for (r_idx, row) in rows.iter().enumerate() {
        events.reading(r_idx, c_idx, row.is_high(), now).await;
    }
}
