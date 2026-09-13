# Larger keyboard: 7 octaves × fifthspan 50

Feasibility assessment saved September 12, 2026, against commit
`ee68e6a654281d804561bd37a6d07c5cd6d7d9c4`.

Status: exploratory findings only. No PCB, schematic, firmware, or mechanical
design changes have been made for this revision. The user is considering returning
to this project later; this document is the starting context for that work.

## Intent and assessment

Expand the existing Wicki-Hayden mechanical-switch MIDI keyboard from 5 octaves
and fifthspan 25 to 7 octaves and fifthspan 50. “50 notes per octave” is assumed to
mean 50 consecutive positions on the chain of fifths, following the existing
README, rather than necessarily 50 equal divisions of the octave.

This appears feasible as a medium-sized hardware revision. Existing key circuitry,
footprints, routing templates, and the RP2040 controller are useful starting
points. Expect several focused design/review sessions for a first reviewable
fabrication candidate, then manufacturing and physical bring-up, with allowance
for another PCB revision. This is a preliminary effort estimate, not a validated
schedule or a guarantee that the first manufactured board will work.

| Item | Existing board | Candidate larger board |
|---|---|---|
| Key positions | 125 nominal; 123 populated | About 350, depending on controller space |
| Scan matrix | 10 rows × 13 columns | Potentially 14 rows × 25 columns |
| Column shift registers | Two 74HC595s | Four 74HC595s, with spare outputs |
| Controller | RP2040-Zero | Likely reusable |
| Size | PCB outline about 209 × 197 mm; case about 21 × 22 cm | Roughly 40 × 28 cm at existing spacing; unverified estimate |

The proposed matrix dimensions describe electrical scanning, not a finalized
physical key arrangement. Exact fifthspan boundaries and the change from an odd
to an even fifthspan need to be worked out when generating the geometry.

## Verified baseline

- Main design: [pcb/5x25](../pcb/5x25/), KiCad 9 schematic and PCB.
- Four copper layers; the PCB includes a 5 V inner-layer zone and ground zones.
- PCB footprint inventory: 123 switches, 123 LEDs, 124 diodes, 125 capacitors,
  two 74HC595s, one RP2040-Zero, one resistor, and nine mounting holes.
- Two musical key positions are omitted for the controller.
- The controller currently uses ten row inputs, three shift-register control
  pins, and one LED data pin. Six footprint GPIO pins are unconnected.
- KiCad DRC with schematic parity checking reported zero unconnected items,
  zero schematic parity issues, and four warnings. All four warnings were
  capacitor footprints differing from their library copies: C14, C63, C88, C113.
  This establishes a useful baseline; it does not verify electrical performance.

Reproduce the baseline check from the repository root:

```sh
kicad-cli pcb drc --format json --schematic-parity \
  -o /tmp/lattice-5x25-drc.json pcb/5x25/5x25.kicad_pcb
```

On the inspected Mac, the CLI was at
`/Applications/KiCad/KiCad.app/Contents/MacOS/kicad-cli`.

## PCB and mechanical work

1. **Generate the geometry from parameters.**
   [lattice_plugins.py](../pcb/5x25/lattice_plugins.py) already clones local
   component placement and trace templates, but hardcodes 125 keys, groups of 25,
   five groups, and specific row/column/LED connections. It is a useful source of
   reusable patterns, not a complete arbitrary-size board generator. Prefer one
   authoritative layout definition that generates PCB placement and firmware
   mappings, with explicit treatment of edge keys and omitted positions.
2. **Expand the matrix and validate scanning.**
   A candidate 14×25 matrix needs 14 input pins plus three shift-register control
   pins and one LED pin: 18 GPIOs. This fits the RP2040-Zero's 20 edge GPIOs.
   Four cascaded 8-bit 595s provide enough column outputs. Pin assignment,
   initialization, scan timing, and long interconnect behavior still need review.
3. **Establish an LED power budget.**
   At comparable lighting settings, LED load scales to approximately 2.8 times
   the current board. Measure or verify the actual LED current, decide maximum
   brightness, then design the supply path, copper distribution, decoupling,
   input protection, and logic-level interface accordingly. USB-only power versus
   a separate supply remains undecided.
4. **Update the enclosure and mounting scheme.**
   The much wider PCB needs appropriate mechanical support. Compare a single
   PCB with connected sections if fabrication or enclosure constraints justify
   that complexity. Local mechanical files in [models](../models/) are STEP/3MF
   exports; the README links the original Onshape models. The editable Onshape
   design was not inspected in this assessment.

Hardware references checked during the assessment:
[Waveshare RP2040-Zero documentation](https://www.waveshare.com/wiki/RP2040-Zero)
and [TI SN74HC595 documentation](https://www.ti.com/product/SN74HC595).

## Firmware findings to carry forward

- **LED indices must be widened.**
  [layout_5x25.rs](../firmware/controller/src/layouts/layout_5x25.rs) uses `u8`
  entries and reserves `255` for `NO_LED`; indices for 350 LEDs cannot fit.
  The generic reverse-lookup helper in
  [core/layout.rs](../firmware/core/src/layout.rs) also accepts `u8` entries.
  Use a wider representation and an unambiguous missing-entry value.
- **Unify LED counts.** The layout declares 123 LEDs, while
  [leds.rs](../firmware/controller/src/leds.rs) sends a 125-LED buffer.
- **Add a layout and update row pin assignments.** The scanner in
  [keys/shift_reg.rs](../firmware/controller/src/keys/shift_reg.rs) loops over
  layout constants, but the current row-pin macro names exactly ten pins.
  RAM for a few hundred keys and LEDs appears modest; actual scan latency and
  CPU load have not been benchmarked.
- **Review LED refresh cost.** At an assumed 800 kHz, transmitting 350 RGB LEDs
  takes about 10.5 ms before reset time, while the task requests a 2 ms tick.
  Lighting also repeatedly searches the board for pitches per active voice.
  Choose a realistic refresh rate and consider cached or change-driven updates.
- **Review event bursts and held-key capacities.** The scanner drops events if
  its 32-entry MIDI queue is full, potentially losing note-offs. Local held-key
  visualization has a 16-entry capacity; other voice/highlight collections have
  32-entry limits. More total keys do not inherently require more polyphony, but
  the intended simultaneous-key behavior and debounce should be checked.
- **Preserve the distinction between MIDI modes.** In the inspected
  [tuning.rs](../firmware/controller/src/tuning.rs), `Fifths` is the default and
  encodes octave/fifth offsets as channel/note numbers, clamped to 16 channels
  and 128 notes. Validate the new layout center and all edge coordinates to avoid
  clamping aliases. `Standard` with a non-700-cent fifth uses the MPE allocator,
  which has 15 note channels; that limit is simultaneous bent voices, not total
  keyboard keys. Do not assume the README fully describes current behavior.

## Suggested restart point

First generate a reviewable 7×50 physical key layout and board outline, preserving
the existing switch/keycap geometry. Confirm the exact fifthspan, musical range,
controller location, hand reach, and desk footprint before detailed routing.

Then settle power and single-board versus sectional construction; produce the
schematic, placement, routing, mounting, and firmware mappings from the agreed
layout. Verify mapping uniqueness and coverage, DRC/ERC and schematic parity,
firmware builds, and power/mechanical assumptions before preparing fabrication
files. Physical bring-up should exercise every key and LED, simultaneous presses,
release-event reliability, scan latency, and voltage drop at maximum permitted
brightness.

No budget, power architecture, sectional construction, or exact outline has been
approved yet. Do not treat the candidate dimensions or scan matrix as final design
requirements.
