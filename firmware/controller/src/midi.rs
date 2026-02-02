use core::cell::{Cell, RefCell};
use embassy_futures::join::join;
use embassy_rp::peripherals::USB;
use embassy_rp::usb::Driver as UsbDriver;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_time::{with_timeout, Duration, Timer};
use embassy_usb::class::midi::MidiClass;
use heapless::Vec;
use log::{error, info};
use wmidi::*;

// ----------------------------------------------------------------------------
// Remote Voice Tracking (for LED Visualization)
// ----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RemoteVoice {
    pub channel: Channel,
    pub note: Note,
    pub velocity: U7,
    pub pitch_bend: u16, // Raw 14-bit value (0-16383, center 8192)
}

pub static REMOTE_VOICES: Mutex<
    CriticalSectionRawMutex,
    RefCell<Vec<RemoteVoice, 32>>, // Support polyphony
> = Mutex::new(RefCell::new(Vec::new()));

pub static CHANNEL_BENDS: Mutex<CriticalSectionRawMutex, Cell<[u16; 16]>> =
    Mutex::new(Cell::new([8192u16; 16]));

// ----------------------------------------------------------------------------
// MIDI Task Types
// ----------------------------------------------------------------------------

// Define a local trait to add functionality to u8
pub trait ToU7 {
    fn to_u7(self) -> U7;
}

// Implement the trait for u8
impl ToU7 for u8 {
    fn to_u7(self) -> U7 {
        U7::new(self.min(127)).unwrap()
    }
}

// Define the event type for inter-task communication
#[derive(Debug, Clone, Copy)]
pub enum MidiEvent {
    NoteOn {
        channel: wmidi::Channel,
        note: Note,
        velocity: U7,
    },
    NoteOff {
        channel: wmidi::Channel,
        note: Note,
        velocity: U7,
    },
    #[allow(dead_code)]
    PitchBendChange {
        channel: wmidi::Channel,
        value: u16, // 14-bit value (0-16383, center 8192)
    },
    MpeNoteOn {
        channel: wmidi::Channel,
        note: Note,
        velocity: U7,
        pitch_bend: u16,
    },
}

#[embassy_executor::task]
pub async fn midi_task(
    midi: MidiClass<'static, UsbDriver<'static, USB>>,
    receiver: embassy_sync::channel::Receiver<
        'static,
        embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex,
        MidiEvent,
        32,
    >,
) {
    // Wait a moment for USB to settle
    Timer::after(Duration::from_millis(1000)).await;
    info!("MIDI Task Started!");

    let (mut sender, mut rx) = midi.split();

    let send_future = async {
        loop {
            let event = receiver.receive().await;

            match event {
                MidiEvent::NoteOn {
                    channel,
                    note,
                    velocity,
                } => {
                    // Send Pitch Bend Reset (8192) first to ensure no lingering MPE bend affects this note
                    let pb_reset = MidiMessage::PitchBendChange(
                        channel,
                        wmidi::U14::try_from(8192u16).unwrap(),
                    );
                    try_send_midi_message(&mut sender, &pb_reset).await;

                    let msg = MidiMessage::NoteOn(channel, note, velocity);
                    try_send_midi_message(&mut sender, &msg).await;
                }
                MidiEvent::NoteOff {
                    channel,
                    note,
                    velocity,
                } => {
                    let msg = MidiMessage::NoteOff(channel, note, velocity);
                    try_send_midi_message(&mut sender, &msg).await;
                }
                MidiEvent::PitchBendChange { channel, value } => {
                    let msg = MidiMessage::PitchBendChange(
                        channel,
                        wmidi::U14::try_from(value.clamp(0, 16383)).unwrap(),
                    );
                    try_send_midi_message(&mut sender, &msg).await;
                }
                MidiEvent::MpeNoteOn {
                    channel,
                    note,
                    velocity,
                    pitch_bend,
                } => {
                    // Send Pitch Bend first
                    let pb_msg = MidiMessage::PitchBendChange(
                        channel,
                        wmidi::U14::try_from(pitch_bend.clamp(0, 16383)).unwrap(),
                    );
                    try_send_midi_message(&mut sender, &pb_msg).await;

                    // Then Note On
                    let note_msg = MidiMessage::NoteOn(channel, note, velocity);
                    try_send_midi_message(&mut sender, &note_msg).await;
                }
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
                            match wmidi::MidiMessage::try_from(&chunk[1..]) {
                                Ok(message) => {
                                    process_remote_midi(&message);
                                }
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

pub fn channel_to_index(ch: Channel) -> usize {
    match ch {
        Channel::Ch1 => 0,
        Channel::Ch2 => 1,
        Channel::Ch3 => 2,
        Channel::Ch4 => 3,
        Channel::Ch5 => 4,
        Channel::Ch6 => 5,
        Channel::Ch7 => 6,
        Channel::Ch8 => 7,
        Channel::Ch9 => 8,
        Channel::Ch10 => 9,
        Channel::Ch11 => 10,
        Channel::Ch12 => 11,
        Channel::Ch13 => 12,
        Channel::Ch14 => 13,
        Channel::Ch15 => 14,
        Channel::Ch16 => 15,
    }
}

pub fn index_to_channel(idx: u8) -> Option<Channel> {
    match idx {
        0 => Some(Channel::Ch1),
        1 => Some(Channel::Ch2),
        2 => Some(Channel::Ch3),
        3 => Some(Channel::Ch4),
        4 => Some(Channel::Ch5),
        5 => Some(Channel::Ch6),
        6 => Some(Channel::Ch7),
        7 => Some(Channel::Ch8),
        8 => Some(Channel::Ch9),
        9 => Some(Channel::Ch10),
        10 => Some(Channel::Ch11),
        11 => Some(Channel::Ch12),
        12 => Some(Channel::Ch13),
        13 => Some(Channel::Ch14),
        14 => Some(Channel::Ch15),
        15 => Some(Channel::Ch16),
        _ => None,
    }
}

// ----------------------------------------------------------------------------
// Remote Voice Tracking (for LED Visualization)
// ----------------------------------------------------------------------------

fn process_remote_midi(message: &MidiMessage) {
    match message {
        MidiMessage::NoteOn(ch, note, vel) => {
            let velocity: u8 = (*vel).into();
            if velocity > 0 {
                let initial_bend = CHANNEL_BENDS.lock(|b| b.get()[channel_to_index(*ch)]);
                REMOTE_VOICES.lock(|v| {
                    let mut voices = v.borrow_mut();
                    if let Some(existing) = voices
                        .iter_mut()
                        .find(|v| v.channel == *ch && v.note == *note)
                    {
                        existing.velocity = *vel;
                        existing.pitch_bend = initial_bend;
                    } else {
                        let _ = voices.push(RemoteVoice {
                            channel: *ch,
                            note: *note,
                            velocity: *vel,
                            pitch_bend: initial_bend,
                        });
                    }
                });
            } else {
                REMOTE_VOICES.lock(|v| {
                    v.borrow_mut()
                        .retain(|v| !(v.channel == *ch && v.note == *note));
                });
            }
        }
        MidiMessage::NoteOff(ch, note, _vel) => {
            REMOTE_VOICES.lock(|v| {
                v.borrow_mut()
                    .retain(|v| !(v.channel == *ch && v.note == *note));
            });
        }
        MidiMessage::PitchBendChange(ch, bend) => {
            let bend_val: u16 = (*bend).into();
            CHANNEL_BENDS.lock(|b| {
                let mut bends = b.get();
                bends[channel_to_index(*ch)] = bend_val;
                b.set(bends);
            });
            REMOTE_VOICES.lock(|v| {
                for voice in v.borrow_mut().iter_mut() {
                    if voice.channel == *ch {
                        voice.pitch_bend = bend_val;
                    }
                }
            });
        }
        MidiMessage::ControlChange(_ch, cc, _val) => {
            let cc_num: u8 = (*cc).into();
            if cc_num == 120 || cc_num == 123 {
                REMOTE_VOICES.lock(|v| v.borrow_mut().clear());
            }
        }
        _ => {}
    }
}

async fn try_send_midi_message(
    sender: &mut embassy_usb::class::midi::Sender<'static, UsbDriver<'static, USB>>,
    message: &wmidi::MidiMessage<'_>,
) {
    let mut buf = [0u8; 3];
    if message.copy_to_slice(&mut buf).is_err() {
        error!("Buffer copy error while sending {:?}", message);
        return;
    }

    let cin = match message {
        wmidi::MidiMessage::NoteOff(..) => 0x08,
        wmidi::MidiMessage::NoteOn(..) => 0x09,
        wmidi::MidiMessage::PolyphonicKeyPressure(..) => 0x0A,
        wmidi::MidiMessage::ControlChange(..) => 0x0B,
        wmidi::MidiMessage::ProgramChange(..) => 0x0C,
        wmidi::MidiMessage::ChannelPressure(..) => 0x0D,
        wmidi::MidiMessage::PitchBendChange(..) => 0x0E,
        _ => 0x0F,
    };

    let packet = [cin, buf[0], buf[1], buf[2]];

    match with_timeout(Duration::from_millis(10), sender.write_packet(&packet)).await {
        Ok(Ok(_)) => {}
        Ok(Err(_)) => error!(
            "Packet write failure (USB Error) while sending {:?}",
            message
        ),
        Err(_) => {
            error!(
                "Packet write timeout (Host stalled?) while sending {:?}",
                message
            );
        }
    }
}
