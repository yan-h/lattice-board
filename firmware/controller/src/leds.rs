use core::cell::RefCell;
use embassy_rp::pio::Pio;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_time::Duration;
use heapless::Vec;
use lattice_board_core::layout::{Coordinate, Layout};
use smart_leds::RGB8;

use crate::keys::ACTIVE_KEYS;
use crate::layouts::{COLS, ROWS};
use crate::midi::REMOTE_VOICES;
use crate::tuning::{get_mpe_pbr, PITCH_ANCHOR_CENTS};

pub struct LedConfig {
    pub brightness: f32, // Global brightness (0-1)
    pub hue_offset: f32, // Input rotation
    pub rgb_anchors: [RGB8; 12],
    pub selected_anchor: usize,
}

pub static LED_CONFIG: Mutex<CriticalSectionRawMutex, RefCell<LedConfig>> =
    Mutex::new(RefCell::new(LedConfig {
        brightness: 0.05,
        hue_offset: 0.0,
        // Standard 12-tone Rainbow as default
        rgb_anchors: [
            RGB8::new(255, 5, 5),   // 0: Red
            RGB8::new(225, 35, 0),  // 1: Orange
            RGB8::new(210, 75, 0),  // 2: Yellow
            RGB8::new(175, 130, 0), // 3: Yellow green
            RGB8::new(90, 220, 0),  // 4: Green
            RGB8::new(0, 245, 35),  // 5: Spring Green
            RGB8::new(0, 165, 130), // 6: Cyan
            RGB8::new(0, 80, 200),  // 7: Azure
            RGB8::new(20, 20, 245), // 8: Blue
            RGB8::new(100, 0, 200), // 9: Purple
            RGB8::new(200, 0, 100), // 10: Magenta
            RGB8::new(215, 0, 25),  // 11: Rose
        ],
        selected_anchor: 0,
    }));

#[cfg(feature = "layout-5x25")]
type LedPin = embassy_rp::peripherals::PIN_3;
#[cfg(feature = "layout-prototype")]
type LedPin = embassy_rp::peripherals::PIN_29;

#[cfg(feature = "layout-5x25")]
use crate::layouts::layout_5x25::Layout5x25 as CurrentLayout;
#[cfg(feature = "layout-5x25")]
const NUM_LEDS: usize = 125;

#[cfg(feature = "layout-prototype")]
use crate::layouts::prototype::PrototypeLayout as CurrentLayout;
#[cfg(feature = "layout-prototype")]
const NUM_LEDS: usize = 20;

use embassy_time::Ticker;

#[embassy_executor::task]
pub async fn led_task(
    mut pio: Pio<'static, embassy_rp::peripherals::PIO0>,
    pin: LedPin,
    dma: embassy_rp::peripherals::DMA_CH0,
) {
    use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program};

    // Load the program
    let program = PioWs2812Program::new(&mut pio.common);

    // Configure State Machine
    let mut ws2812 = PioWs2812::new(&mut pio.common, pio.sm0, dma, pin, &program);

    // Buffer: NUM_LEDS (RGB8)
    let mut data = [RGB8::default(); NUM_LEDS];
    let mut ticker = Ticker::every(Duration::from_millis(2));

    loop {
        ticker.next().await;

        // Read config
        let (brightness, h_offset, anchors) = LED_CONFIG.lock(|c| {
            let config = c.borrow();
            (config.brightness, config.hue_offset, config.rgb_anchors)
        });

        // Resolve All Active Coordinates (Local + Remote)
        let mut active_lit: Vec<Coordinate, 32> = Vec::new();
        // 1. Local (Physical) Keys: Find all enharmonic equivalents
        ACTIVE_KEYS.lock(|k| {
            for &coord in k.borrow().iter() {
                let pitch_cents = crate::tuning::get_key_pitch::<CurrentLayout>(coord);

                let candidates = crate::tuning::find_closest_keys::<CurrentLayout>(
                    pitch_cents,
                    200.0,
                    ROWS,
                    COLS,
                    None, // No MIDI note bias for local keys
                );

                for c in candidates {
                    if !active_lit.contains(&c) {
                        let _ = active_lit.push(c);
                    }
                }
            }
        });

        // 2. Remote (MIDI) Voices
        REMOTE_VOICES.lock(|v| {
            for voice in v.borrow().iter() {
                // Calculate target cents relative to PITCH_ANCHOR_CENTS
                let bend_val = voice.pitch_bend as f32;
                let mpe_pbr = get_mpe_pbr();
                let bend_semitones = (bend_val - 8192.0) / (8192.0 / mpe_pbr);

                let target_cents = ((u8::from(voice.note) as f32 - 60.0) * 100.0)
                    + PITCH_ANCHOR_CENTS
                    + (bend_semitones * 100.0);

                let candidates = crate::tuning::find_closest_keys::<CurrentLayout>(
                    target_cents,
                    200.0,
                    ROWS,
                    COLS,
                    Some(u8::from(voice.note)),
                );

                for coord in candidates {
                    if !active_lit.contains(&coord) {
                        let _ = active_lit.push(coord);
                    }
                }
            }
        });

        for i in 0..NUM_LEDS {
            // Get logical coordinate for this LED
            if let Some(coord) = CurrentLayout::led_to_coord(i) {
                // Get center coordinate for relative calculation
                let center = CurrentLayout::center_coord();
                let dx = coord.x as i32 - center.x as i32;
                let dy = coord.y as i32 - center.y as i32;

                // Calculate semitone position (0-11) relative to center
                // x (Major 2nd, +2 st) = 2 Fifths
                // y (Desc 4th, -5 st) = 1 Fifth
                // Center matches Red (Color 0)
                let fifths = (dx * 2) + (dy * 1);
                let notes = (fifths * 7).rem_euclid(12); // 0..11 integer semitone
                let _notes2 = fifths.rem_euclid(12);

                // Add offset. Assuming h_offset is in degrees (0..360), map to 0..12
                let offset_semitones = h_offset / 30.0;
                let position = (notes as f32 + offset_semitones) % 12.0;

                // Interpolate
                let idx = position as usize; // 0..11
                let t = position - idx as f32; // 0.0..1.0

                let next_idx = (idx + 1) % 12;

                let c1 = anchors[idx];
                let c2 = anchors[next_idx];

                // Linear RGB Interpolation
                // We cast to f32 to do the math, then scale and cast back to u8
                let mut r_f = c1.r as f32 + (c2.r as f32 - c1.r as f32) * t;
                let mut g_f = c1.g as f32 + (c2.g as f32 - c1.g as f32) * t;
                let mut b_f = c1.b as f32 + (c2.b as f32 - c1.b as f32) * t;

                // Scale by global brightness
                let mut scale = brightness;

                // Check if this LED should be lit by any active interaction (held keys)
                if active_lit.contains(&coord) {
                    // Move 1/3 of the way towards white (255)
                    r_f = r_f + (255.0 - r_f) * 0.6;
                    g_f = g_f + (255.0 - g_f) * 0.6;
                    b_f = b_f + (255.0 - b_f) * 0.6;

                    // Double the brightness
                    scale *= 3.0;
                }

                let r = (r_f * scale).min(255.0) as u8;
                let g = (g_f * scale).min(255.0) as u8;
                let b = (b_f * scale).min(255.0) as u8;

                data[i] = RGB8::new(r, g, b);
            } else {
                let v = (50.0 * brightness) as u8;
                data[i] = RGB8::new(v, v, v);
            }
        }

        ws2812.write(&data).await;
    }
}
