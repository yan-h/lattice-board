use core::cell::RefCell;
use embassy_futures::join::join;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::Driver as UsbDriver;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::channel::{Receiver, Sender};
use embassy_time::{with_timeout, Duration, Timer};
use embassy_usb::class::midi::MidiClass;
use heapless::Vec;
use log::{error, info};
use wmidi::{Channel, MidiMessage, Note};

/// A key's note turning on or off.
#[derive(Debug, Clone, Copy)]
pub struct NoteEvent {
    pub channel: Channel,
    pub note: Note,
    pub on: bool,
}

pub const NOTE_QUEUE_LEN: usize = 32;
pub type NoteSender = Sender<'static, CriticalSectionRawMutex, NoteEvent, NOTE_QUEUE_LEN>;
pub type NoteReceiver = Receiver<'static, CriticalSectionRawMutex, NoteEvent, NOTE_QUEUE_LEN>;

const VELOCITY: u8 = 100;

/// Bytes per USB packet: 15 four-byte USB-MIDI events. Staying under the
/// 64-byte maximum keeps every packet short, so the host delivers it at once
/// rather than waiting for the transfer to continue.
const PACKET_LEN: usize = 60;

/// Notes the host is playing back to us, for the LEDs.
pub static REMOTE_VOICES: Mutex<CriticalSectionRawMutex, RefCell<Vec<(Channel, Note), 32>>> =
    Mutex::new(RefCell::new(Vec::new()));

#[embassy_executor::task]
pub async fn midi_task(midi: MidiClass<'static, UsbDriver<'static, USB>>, receiver: NoteReceiver) {
    // Wait a moment for USB to settle
    Timer::after(Duration::from_millis(1000)).await;
    info!("MIDI Task Started!");

    let (mut sender, mut rx) = midi.split();

    // Sends each event as soon as it arrives, along with any queued behind
    // it, so a chord goes out in one packet.
    let send_future = async {
        let mut packet = [0u8; PACKET_LEN];
        loop {
            let mut len = 0;
            let mut event = receiver.receive().await;
            loop {
                packet[len..len + 4].copy_from_slice(&usb_midi_event(event));
                len += 4;
                if len == PACKET_LEN {
                    break;
                }
                match receiver.try_receive() {
                    Ok(next) => event = next,
                    Err(_) => break,
                }
            }

            match with_timeout(Duration::from_millis(10), sender.write_packet(&packet[..len]))
                .await
            {
                Ok(Ok(())) => {}
                Ok(Err(_)) => error!("MIDI packet write failed (USB error)"),
                Err(_) => error!("MIDI packet write timed out (host stalled?)"),
            }
        }
    };

    let receive_future = async {
        let mut buf = [0u8; 64];
        loop {
            match rx.read_packet(&mut buf).await {
                Ok(n) => {
                    for chunk in buf[..n].chunks(4) {
                        if chunk.len() == 4 && chunk[0] != 0 {
                            match MidiMessage::try_from(&chunk[1..]) {
                                Ok(message) => process_remote_midi(&message),
                                Err(_) => info!("Received Raw: {:?}", chunk),
                            }
                        }
                    }
                }
                Err(_e) => {
                    info!("MIDI Read Error");
                }
            }
        }
    };

    join(send_future, receive_future).await;
}

/// One USB-MIDI event packet on cable 0: code index number, then the MIDI
/// status and data bytes.
fn usb_midi_event(event: NoteEvent) -> [u8; 4] {
    let (cin, status) = if event.on { (0x09, 0x90) } else { (0x08, 0x80) };
    [
        cin,
        status | event.channel.index(),
        u8::from(event.note),
        VELOCITY,
    ]
}

fn process_remote_midi(message: &MidiMessage) {
    match *message {
        MidiMessage::NoteOn(ch, note, vel) if u8::from(vel) > 0 => {
            REMOTE_VOICES.lock(|v| {
                let mut voices = v.borrow_mut();
                if !voices.contains(&(ch, note)) {
                    let _ = voices.push((ch, note));
                }
            });
        }
        MidiMessage::NoteOn(ch, note, _) | MidiMessage::NoteOff(ch, note, _) => {
            REMOTE_VOICES.lock(|v| v.borrow_mut().retain(|&voice| voice != (ch, note)));
        }
        MidiMessage::ControlChange(_, cc, _) => {
            let cc_num: u8 = cc.into();
            if cc_num == 120 || cc_num == 123 {
                REMOTE_VOICES.lock(|v| v.borrow_mut().clear());
            }
        }
        _ => {}
    }
}
