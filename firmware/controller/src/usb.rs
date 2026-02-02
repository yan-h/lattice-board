use crate::layouts::CurrentLayout;
use core::cell::RefCell;
use core::pin::pin;
use embassy_futures::select::{select, Either};
use embassy_rp::peripherals;
use embassy_rp::rom_data::reset_to_usb_boot;
use embassy_rp::usb::Driver;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_acm::CdcAcmClass;
use log::info;

#[derive(PartialEq, Copy, Clone)]
enum SerialState {
    Log,
    Dashboard,
}

static SERIAL_STATE: Mutex<CriticalSectionRawMutex, RefCell<SerialState>> =
    Mutex::new(RefCell::new(SerialState::Log));

pub static LOG_PIPE: embassy_sync::pipe::Pipe<CriticalSectionRawMutex, 1024> =
    embassy_sync::pipe::Pipe::new();

const CURSOR_HOME: &[u8] = b"\x1B[H";
const CLEAR_SCREEN: &[u8] = b"\x1B[2J";
const HIDE_CURSOR: &[u8] = b"\x1B[?25l";
const SHOW_CURSOR: &[u8] = b"\x1B[?25h";

#[embassy_executor::task]
pub async fn usb_task(
    mut device: embassy_usb::UsbDevice<'static, Driver<'static, peripherals::USB>>,
) {
    device.run().await;
}

#[embassy_executor::task]
pub async fn serial_task(mut class: CdcAcmClass<'static, Driver<'static, peripherals::USB>>) {
    loop {
        class.wait_connection().await;
        info!("Serial connected");
        let _ = serial_process(&mut class).await;
        info!("Serial disconnected");
    }
}

async fn serial_process(
    class: &mut CdcAcmClass<'static, Driver<'static, peripherals::USB>>,
) -> Result<(), ()> {
    let mut buf = [0u8; 64];
    let mut log_buf = [0u8; 64];

    loop {
        let mut result_n = None;
        let mut result_log = None;
        let mut result_tick = false;

        {
            let read_fut = class.read_packet(&mut buf);
            let log_read_fut = LOG_PIPE.read(&mut log_buf);
            let ticker = Timer::after(Duration::from_millis(100));

            let read_fut = pin!(read_fut);
            let log_read_fut = pin!(log_read_fut);
            let ticker = pin!(ticker);

            let combined = select(read_fut, select(log_read_fut, ticker));

            match combined.await {
                Either::First(res) => {
                    if let Ok(n) = res {
                        result_n = Some(n);
                    } else {
                        return Err(());
                    }
                }
                Either::Second(Either::First(n)) => {
                    result_log = Some(n);
                }
                Either::Second(Either::Second(_)) => {
                    result_tick = true;
                }
            }
        }

        if let Some(n) = result_n {
            let data = &buf[..n];
            let mut state = SERIAL_STATE.lock(|s| *s.borrow());

            for &b in data {
                if b == b'D' || b == b'd' {
                    state = if state == SerialState::Log {
                        let _ = class.write_packet(CLEAR_SCREEN).await;
                        let _ = class.write_packet(HIDE_CURSOR).await;
                        SerialState::Dashboard
                    } else {
                        let _ = class.write_packet(SHOW_CURSOR).await;
                        let _ = class.write_packet(b"\r\n--- Log Mode ---\r\n").await;
                        SerialState::Log
                    };
                    SERIAL_STATE.lock(|s| *s.borrow_mut() = state);
                }
            }

            if state == SerialState::Log {
                let _ = class.write_packet(data).await;
            }

            crate::leds::LED_CONFIG.lock(|c| {
                let mut config = c.borrow_mut();
                let clamp_u8 =
                    |v: u8, delta: i16| -> u8 { ((v as i16 + delta).max(0).min(255)) as u8 };
                for &b in data {
                    let sel = config.selected_anchor;
                    let mut rgb = config.rgb_anchors[sel];
                    match b {
                        b'[' => config.selected_anchor = (config.selected_anchor + 11) % 12,
                        b']' => config.selected_anchor = (config.selected_anchor + 1) % 12,
                        b'r' => rgb.r = clamp_u8(rgb.r, -5),
                        b'R' => rgb.r = clamp_u8(rgb.r, 5),
                        b'g' => rgb.g = clamp_u8(rgb.g, -5),
                        b'G' => rgb.g = clamp_u8(rgb.g, 5),
                        b'b' => rgb.b = clamp_u8(rgb.b, -5),
                        b'B' => rgb.b = clamp_u8(rgb.b, 5),
                        b'L' => config.brightness = (config.brightness + 0.05).min(1.0),
                        b'l' => config.brightness = (config.brightness - 0.05).max(0.0),
                        b'+' | b'=' => config.brightness = (config.brightness + 0.01).min(1.0),
                        b'-' | b'_' => config.brightness = (config.brightness - 0.01).max(0.0),
                        b'H' => config.hue_offset = (config.hue_offset + 1.0) % 360.0,
                        b'h' => config.hue_offset = (config.hue_offset - 1.0 + 360.0) % 360.0,
                        b't' | b'T' => {
                            let _ = crate::tuning::toggle_mode();
                        }
                        b'(' => crate::tuning::adjust_fifth_size(-1.0),
                        b')' => crate::tuning::adjust_fifth_size(1.0),
                        b'{' => crate::tuning::adjust_fifth_size(-0.1),
                        b'}' => crate::tuning::adjust_fifth_size(0.1),
                        b',' => crate::tuning::adjust_mpe_pbr(-1.0),
                        b'.' => crate::tuning::adjust_mpe_pbr(1.0),
                        b'<' => crate::tuning::adjust_mpe_pbr(-0.1),
                        b'>' => crate::tuning::adjust_mpe_pbr(0.1),
                        _ => {}
                    }
                    config.rgb_anchors[sel] = rgb;
                }
            });
        }

        if let Some(n) = result_log {
            let state = SERIAL_STATE.lock(|s| *s.borrow());
            if state == SerialState::Log {
                let _ = class.write_packet(&log_buf[..n]).await;
            }
        }

        if result_tick {
            let state = SERIAL_STATE.lock(|s| *s.borrow());
            if state == SerialState::Dashboard {
                draw_dashboard(class).await;
            }
        }

        check_for_reset(class).await;
    }
}

async fn draw_dashboard(class: &mut CdcAcmClass<'static, Driver<'static, peripherals::USB>>) {
    use core::fmt::Write;
    let mut out: heapless::String<1024> = heapless::String::new();

    let (b, h, sel, anchors, mode, size, pbr) = crate::leds::LED_CONFIG.lock(|cfg| {
        let cfg = cfg.borrow();
        let m = crate::tuning::get_mode();
        let s = crate::tuning::get_fifth_size();
        let p = crate::tuning::get_mpe_pbr();
        (
            cfg.brightness,
            cfg.hue_offset,
            cfg.selected_anchor,
            cfg.rgb_anchors,
            m,
            s,
            p,
        )
    });

    let active_keys = crate::keys::ACTIVE_KEYS.lock(|c| c.borrow().clone());

    let _ = class.write_packet(CURSOR_HOME).await;
    let rgb = anchors[sel];
    let _ = write!(
        out,
        "Lattice Board Controller v0.1.0\x1B[K\r\n\
         -------------------------------\x1B[K\r\n\
         Brightness: {:.2} | Hue: {:.0} | Mode: {:?}\x1B[K\r\n\
         Fifth: {:.1}c | PBR: {:.1}\x1B[K\r\n\
         RGB: Idx {} | R{} G{} B{}\x1B[K\r\n\r\n\
         Held Keys:\x1B[K\r\n",
        b, h, mode, size, pbr, sel, rgb.r, rgb.g, rgb.b
    );

    if active_keys.is_empty() {
        let _ = write!(out, " (None)\x1B[K\r\n");
    } else {
        for k in active_keys {
            let (octaves, fifths) = crate::tuning::calculate_fifths_offsets::<CurrentLayout>(k);
            let _ = write!(out, "Oc:{} F:{} | ", octaves, fifths);
        }
        let _ = write!(out, "\x1B[K\r\n");
    }

    let _ = write!(out, "\r\nRemote MIDI:\x1B[K\r\n");
    crate::midi::REMOTE_VOICES.lock(|v| {
        for voice in v.borrow().iter() {
            let _ = write!(
                out,
                "Ch{} N{} | ",
                crate::midi::channel_to_index(voice.channel) + 1,
                u8::from(voice.note)
            );
        }
    });
    let _ = write!(out, "\x1B[K\r\n");

    for chunk in out.as_bytes().chunks(64) {
        let _ = class.write_packet(chunk).await;
    }
}

async fn check_for_reset(class: &mut CdcAcmClass<'static, Driver<'static, peripherals::USB>>) {
    if class.line_coding().data_rate() == 1200 {
        Timer::after(Duration::from_millis(10)).await;
        reset_to_usb_boot(0, 0);
    }
}
