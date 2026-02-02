use crate::midi::{index_to_channel, MidiEvent};
use crate::mpe::MpeVoiceAllocator;
use core::cell::{Cell, RefCell};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use heapless::Vec;
use lattice_board_core::layout::{Coordinate, Layout};
use micromath::F32Ext;
use wmidi::{Channel, Note, U7};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TuningMode {
    Standard,
    Fifths,
}

pub static CURRENT_TUNING_MODE: Mutex<CriticalSectionRawMutex, Cell<TuningMode>> =
    Mutex::new(Cell::new(TuningMode::Fifths));

static FIFTH_SIZE: Mutex<CriticalSectionRawMutex, Cell<f32>> = Mutex::new(Cell::new(697.0));
static MPE_PBR: Mutex<CriticalSectionRawMutex, Cell<f32>> = Mutex::new(Cell::new(1.0));

pub const PITCH_ANCHOR_CENTS: f32 = 6000.0;

static MPE_ALLOCATOR: Mutex<CriticalSectionRawMutex, RefCell<MpeVoiceAllocator>> =
    Mutex::new(RefCell::new(MpeVoiceAllocator::new()));
static ACTIVE_CHANNELS: Mutex<CriticalSectionRawMutex, RefCell<Vec<(Coordinate, Channel), 16>>> =
    Mutex::new(RefCell::new(Vec::new()));

pub fn toggle_mode() -> TuningMode {
    CURRENT_TUNING_MODE.lock(|m| {
        let new_mode = match m.get() {
            TuningMode::Standard => TuningMode::Fifths,
            TuningMode::Fifths => TuningMode::Standard,
        };
        m.set(new_mode);
        new_mode
    })
}

pub fn get_mode() -> TuningMode {
    CURRENT_TUNING_MODE.lock(|m| m.get())
}

pub fn get_fifth_size() -> f32 {
    FIFTH_SIZE.lock(|f| f.get())
}

pub fn adjust_fifth_size(delta: f32) {
    FIFTH_SIZE.lock(|f| {
        let current = f.get();
        f.set((current + delta).max(600.0).min(800.0));
    });
}

pub fn get_mpe_pbr() -> f32 {
    MPE_PBR.lock(|f| f.get())
}

pub fn adjust_mpe_pbr(delta: f32) {
    MPE_PBR.lock(|f| {
        let current = f.get();
        f.set((current + delta).max(0.1).min(96.0));
    });
}

const FIFTHS_CENTER_CHANNEL: u8 = 4;
const FIFTHS_CENTER_PITCH: u8 = 60;

/// - x + 1, y - 1 (UP-RIGHT) is a Perfect Fifth.
/// - x + 0, y - 2 (UP UP) is an Octave.
pub fn calculate_fifths_offsets<L: Layout>(coord: Coordinate) -> (i16, i16) {
    let center = L::center_coord();
    let dx_raw = coord.x as i16 - center.x as i16;
    let dy_raw = coord.y as i16 - center.y as i16;

    let octaves = (-dy_raw).div_euclid(2);
    let shift = (-dy_raw).rem_euclid(2);
    let fifths = 2 * dx_raw - 2 * octaves - shift;

    (octaves, fifths)
}

pub fn get_midi_event<L: Layout>(
    coord: Coordinate,
    velocity: U7,
    is_note_on: bool,
) -> Option<MidiEvent> {
    let mode = get_mode();
    match mode {
        TuningMode::Standard => {
            if is_note_on {
                let target_cents = get_key_pitch::<L>(coord);
                if get_fifth_size() == 700.0 {
                    let midi_note = ((target_cents / 100.0 + 0.5) as u8).clamp(0, 127);
                    if let Ok(note) = Note::try_from(midi_note) {
                        return Some(MidiEvent::NoteOn {
                            channel: Channel::Ch1,
                            note,
                            velocity,
                        });
                    }
                    return None;
                }
                let channel_opt = MPE_ALLOCATOR.lock(|alloc| alloc.borrow_mut().alloc());
                if let Some(channel) = channel_opt {
                    let _ = ACTIVE_CHANNELS.lock(|chans| chans.borrow_mut().push((coord, channel)));
                    let exact_note_val = target_cents / 100.0;
                    let midi_note = ((exact_note_val + 0.5) as u8).clamp(0, 127);
                    let bend_cents = target_cents - (midi_note as f32 * 100.0);
                    let mpe_pbr = get_mpe_pbr();
                    let bend_units_offset = (bend_cents / 100.0) * (8192.0 / mpe_pbr);
                    let bend_val = (8192.0 + bend_units_offset).clamp(0.0, 16383.0) as u16;
                    if let Ok(note) = Note::try_from(midi_note) {
                        Some(MidiEvent::MpeNoteOn {
                            channel,
                            note,
                            velocity,
                            pitch_bend: bend_val,
                        })
                    } else {
                        MPE_ALLOCATOR.lock(|a| a.borrow_mut().free(channel));
                        ACTIVE_CHANNELS.lock(|c| {
                            let _ = c.borrow_mut().pop();
                        });
                        None
                    }
                } else {
                    None
                }
            } else {
                let found_data = ACTIVE_CHANNELS.lock(|chans| {
                    let mut c = chans.borrow_mut();
                    let mut found = None;
                    for (i, (co, _)) in c.iter().enumerate() {
                        if *co == coord {
                            found = Some(i);
                            break;
                        }
                    }
                    found.map(|idx| c.swap_remove(idx))
                });
                if let Some((_, channel)) = found_data {
                    MPE_ALLOCATOR.lock(|a| a.borrow_mut().free(channel));
                    let target_cents = get_key_pitch::<L>(coord);
                    let midi_note = ((target_cents / 100.0 + 0.5) as u8).clamp(0, 127);
                    if let Ok(note) = Note::try_from(midi_note) {
                        Some(MidiEvent::NoteOff {
                            channel,
                            note,
                            velocity,
                        })
                    } else {
                        None
                    }
                } else if get_fifth_size() == 700.0 {
                    let target_cents = get_key_pitch::<L>(coord);
                    let midi_note = ((target_cents / 100.0 + 0.5) as u8).clamp(0, 127);
                    if let Ok(note) = Note::try_from(midi_note) {
                        Some(MidiEvent::NoteOff {
                            channel: Channel::Ch1,
                            note,
                            velocity,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
        TuningMode::Fifths => {
            let (oc, fifths) = calculate_fifths_offsets::<L>(coord);
            // Spec: Channel increases with physical octaves
            let ch_idx = (FIFTHS_CENTER_CHANNEL as i16 + oc).clamp(0, 15) as u8;
            // Spec: Pitch increases with physical fifths
            let pitch_idx = (FIFTHS_CENTER_PITCH as i16 + fifths).clamp(0, 127) as u8;

            if let Ok(note) = Note::try_from(pitch_idx) {
                let channel = index_to_channel(ch_idx).unwrap_or(Channel::Ch1);
                if is_note_on {
                    Some(MidiEvent::NoteOn {
                        channel,
                        note,
                        velocity,
                    })
                } else {
                    Some(MidiEvent::NoteOff {
                        channel,
                        note,
                        velocity,
                    })
                }
            } else {
                None
            }
        }
    }
}

pub fn get_key_pitch<L: Layout>(coord: Coordinate) -> f32 {
    let (oc, fifths) = calculate_fifths_offsets::<L>(coord);
    // Absolute pitch calculation for standard 12-TET behavior
    // 1 Octave (oc) = 1200 cents
    // 1 Fifth step (fifths) = dynamic fifth size (default 700)
    PITCH_ANCHOR_CENTS + (oc as f32 * 1200.0) + (fifths as f32 * get_fifth_size())
        - (fifths.div_euclid(2) as f32 * 1200.0)
}

pub fn find_closest_keys<L: Layout>(
    target_cents: f32,
    max_dist: f32,
    rows: usize,
    cols: usize,
    bias_note: Option<u8>,
) -> Vec<Coordinate, 4> {
    let mut candidates: Vec<Coordinate, 4> = Vec::new();
    let mut min_dist = max_dist;
    for r in 0..rows {
        for c in 0..cols {
            if let Some(coord) = L::key_to_coord(r, c) {
                let pitch = get_key_pitch::<L>(coord);
                let mut dist = (pitch - target_cents).abs();
                if let Some(note) = bias_note {
                    if L::coord_to_midi(coord) == note {
                        dist -= 20.0;
                    }
                }
                if dist < min_dist {
                    min_dist = dist;
                }
            }
        }
    }
    if min_dist >= max_dist {
        return candidates;
    }
    for r in 0..rows {
        for c in 0..cols {
            if let Some(coord) = L::key_to_coord(r, c) {
                let pitch = get_key_pitch::<L>(coord);
                let mut dist = (pitch - target_cents).abs();
                if let Some(note) = bias_note {
                    if L::coord_to_midi(coord) == note {
                        dist -= 20.0;
                    }
                }
                if dist <= min_dist + 1.0 {
                    let _ = candidates.push(coord);
                    if candidates.is_full() {
                        return candidates;
                    }
                }
            }
        }
    }
    candidates
}
