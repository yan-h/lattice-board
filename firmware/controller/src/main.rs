#![no_std]
#![no_main]

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::{PIO0, USB};
use embassy_rp::pio::Pio;
use embassy_rp::usb::{Driver, InterruptHandler};
use embassy_time::{Duration, Timer};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::class::midi::MidiClass;
use embassy_usb::{Builder, Config};
use log::info;
use panic_probe as _;
use static_cell::StaticCell;

mod keys;
mod layouts;
mod leds;
mod logging;
mod midi;
mod mpe;
mod tuning;
mod usb;
mod util;

pub use lattice_board_core::layout;
pub use lattice_board_core::pitch;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => InterruptHandler<USB>;
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<PIO0>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    let driver = Driver::new(p.USB, Irqs);

    let mut config = Config::new(0x2E8A, 0x000a);
    config.manufacturer = Some("YH");
    config.product = Some("LatticeBoard");

    let uid = util::read_unique_id(p.FLASH);
    static SERIAL_STRING: StaticCell<heapless::String<32>> = StaticCell::new();
    let uid_static = SERIAL_STRING.init(uid);
    config.serial_number = Some(uid_static.as_str());

    static CONFIG_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
    static BOS_DESCRIPTOR: StaticCell<[u8; 256]> = StaticCell::new();
    static CONTROL_BUF: StaticCell<[u8; 64]> = StaticCell::new();
    static STATE: StaticCell<State> = StaticCell::new();

    let mut builder = Builder::new(
        driver,
        config,
        CONFIG_DESCRIPTOR.init([0; 256]),
        BOS_DESCRIPTOR.init([0; 256]),
        &mut [],
        CONTROL_BUF.init([0; 64]),
    );

    let class_cdc = CdcAcmClass::new(&mut builder, STATE.init(State::new()), 64);
    let class_midi = MidiClass::new(&mut builder, 1, 1, 64);

    let usb = builder.build();

    logging::init();
    let pio = Pio::new(p.PIO0, Irqs);

    #[cfg(feature = "layout-5x25")]
    {
        spawner
            .spawn(leds::led_task(pio, p.PIN_3, p.DMA_CH0))
            .unwrap();
    }
    #[cfg(feature = "layout-prototype")]
    {
        spawner
            .spawn(leds::led_task(pio, p.PIN_29, p.DMA_CH0))
            .unwrap();
    }

    spawner.spawn(usb::usb_task(usb)).unwrap();
    spawner.spawn(usb::serial_task(class_cdc)).unwrap();

    static MIDI_CHANNEL: StaticCell<
        embassy_sync::channel::Channel<
            embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
            midi::MidiEvent,
            32,
        >,
    > = StaticCell::new();
    let channel = MIDI_CHANNEL.init(embassy_sync::channel::Channel::new());

    spawner
        .spawn(midi::midi_task(class_midi, channel.receiver()))
        .unwrap();

    use crate::get_rows;

    #[cfg(feature = "layout-5x25")]
    {
        Timer::after(Duration::from_millis(2000)).await;
        crate::layouts::log_key_map();

        let row_pins = get_rows!(p);
        let data_pin = p.PIN_0.into();
        let latch_pin = p.PIN_1.into();
        let clock_pin = p.PIN_2.into();

        spawner
            .spawn(keys::keys_task_shift_reg(
                row_pins,
                data_pin,
                latch_pin,
                clock_pin,
                channel.sender(),
            ))
            .unwrap();
    }

    #[cfg(feature = "layout-prototype")]
    {
        use crate::get_cols;
        let row_pins = get_rows!(p);
        let col_pins = get_cols!(p);
        spawner
            .spawn(keys::keys_task_direct(row_pins, col_pins, channel.sender()))
            .unwrap();
    }

    info!("Controller start. Serial number: {}", uid_static.as_str());

    loop {
        Timer::after(Duration::from_secs(1)).await;
    }
}
