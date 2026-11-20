use core::cell::RefCell;
use embassy_rp::pio::Pio;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_time::{Duration, Ticker};
use lattice_board_core::layout::{decode_fifths_midi, Coordinate, Layout};
use smart_leds::RGB8;

use crate::keys::ACTIVE_KEYS;
use crate::layouts::{CurrentLayout, NUM_LEDS};
use crate::midi::REMOTE_VOICES;

#[derive(Clone, Copy, PartialEq)]
pub struct LedConfig {
    pub brightness: f32, // Global brightness (0-1)
    pub hue_offset: f32, // Input rotation
    pub rgb_anchors: [RGB8; 12],
}

pub static LED_CONFIG: Mutex<CriticalSectionRawMutex, RefCell<LedConfig>> =
    Mutex::new(RefCell::new(LedConfig {
        brightness: 0.14,
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
    }));

#[cfg(feature = "layout-5x25")]
type LedPin = embassy_rp::peripherals::PIN_3;
#[cfg(feature = "layout-prototype")]
type LedPin = embassy_rp::peripherals::PIN_29;

/// Frame period. Sending a 123-LED frame alone takes 3.75 ms.
const FRAME: Duration = Duration::from_millis(10);

#[embassy_executor::task]
pub async fn led_task(
    mut pio: Pio<'static, embassy_rp::peripherals::PIO0>,
    pin: LedPin,
    dma: embassy_rp::peripherals::DMA_CH0,
) {
    use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program};

    let program = PioWs2812Program::new(&mut pio.common);
    let mut ws2812 = PioWs2812::new(&mut pio.common, pio.sm0, dma, pin, &program);

    // Each LED's idle and held colours, recomputed only when the config
    // changes: the float maths is slow without an FPU, and the key scan
    // shares this executor.
    let mut colours_for: Option<LedConfig> = None;
    let mut idle = [RGB8::default(); NUM_LEDS];
    let mut held = [RGB8::default(); NUM_LEDS];

    let mut data = [RGB8::default(); NUM_LEDS];
    let mut ticker = Ticker::every(FRAME);

    loop {
        ticker.next().await;

        let config = LED_CONFIG.lock(|c| *c.borrow());
        if colours_for != Some(config) {
            for i in 0..NUM_LEDS {
                if let Some(coord) = CurrentLayout::led_to_coord(i) {
                    (idle[i], held[i]) = key_colours(coord, &config);
                }
            }
            colours_for = Some(config);
        }

        let mut lit = [false; NUM_LEDS];
        let mut light = |coord: Option<Coordinate>| {
            if let Some(i) = coord.and_then(CurrentLayout::coord_to_led) {
                lit[i] = true;
            }
        };
        ACTIVE_KEYS.lock(|k| k.borrow().iter().for_each(|&c| light(Some(c))));
        REMOTE_VOICES.lock(|v| {
            for &(channel, note) in v.borrow().iter() {
                light(decode_fifths_midi(
                    channel.index(),
                    u8::from(note),
                    CurrentLayout::center_coord(),
                ));
            }
        });

        for i in 0..NUM_LEDS {
            data[i] = if lit[i] { held[i] } else { idle[i] };
        }
        ws2812.write(&data).await;
    }
}

/// A key's colour when idle and when held: its pitch class picks a colour
/// between the twelve anchors, and holding the key whitens and brightens it.
fn key_colours(coord: Coordinate, config: &LedConfig) -> (RGB8, RGB8) {
    let center = CurrentLayout::center_coord();
    let dx = coord.x as i32 - center.x as i32;
    let dy = coord.y as i32 - center.y as i32;

    // x + 1 is 2 fifths and y + 1 is 1 fifth; the center is anchor 0.
    let fifths = dx * 2 + dy;
    let semitones = (fifths * 7).rem_euclid(12);

    // Hue offset is in degrees (0..360); 30 degrees per semitone.
    let position = (semitones as f32 + config.hue_offset / 30.0) % 12.0;
    let idx = position as usize;
    let t = position - idx as f32;
    let c1 = config.rgb_anchors[idx];
    let c2 = config.rgb_anchors[(idx + 1) % 12];
    let lerp = |a: u8, b: u8| a as f32 + (b as f32 - a as f32) * t;
    let rgb = [lerp(c1.r, c2.r), lerp(c1.g, c2.g), lerp(c1.b, c2.b)];

    let scale = |rgb: [f32; 3], s: f32| {
        let ch = |v: f32| (v * s).min(255.0) as u8;
        RGB8::new(ch(rgb[0]), ch(rgb[1]), ch(rgb[2]))
    };
    // Held: 60% of the way to white, at three times the brightness.
    let whitened = rgb.map(|v| v + (255.0 - v) * 0.6);
    (
        scale(rgb, config.brightness),
        scale(whitened, config.brightness * 3.0),
    )
}
