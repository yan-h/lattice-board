use embassy_executor::task;
use embassy_rp::gpio::{AnyPin, Input, Level, Output, Pull};
use embassy_time::{Duration, Instant, Timer};
use log::info;

use super::KeyEvents;
use crate::layouts::{COLS, ROWS};
use crate::midi::NoteSender;

#[task]
pub async fn keys_task_direct(
    row_pins: [AnyPin; ROWS],
    col_pins: [AnyPin; COLS],
    sender: NoteSender,
) {
    // Direct GPIO Scanning
    // Columns are Outputs, Rows are Inputs.
    // Active High: Col set High, Row read High (Pull-Down).
    let rows: [Input<'static>; ROWS] = row_pins.map(|p| Input::new(p, Pull::Down));
    let mut cols: [Output<'static>; COLS] = col_pins.map(|p| Output::new(p, Level::Low));
    let mut events = KeyEvents::new(sender);

    info!("Keys task started. Direct GPIO Scanning.");

    loop {
        for (c_idx, col) in cols.iter_mut().enumerate() {
            col.set_high();
            // Allow signal to settle
            Timer::after(Duration::from_micros(10)).await;

            let now = Instant::now();
            for (r_idx, row) in rows.iter().enumerate() {
                events.reading(r_idx, c_idx, row.is_high(), now).await;
            }

            col.set_low();
        }

        Timer::after(Duration::from_millis(1)).await;
    }
}
