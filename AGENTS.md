# LatticeBoard

Firmware build, flashing and the serial dashboard: [firmware/README.md](firmware/README.md).
`core` holds the pure logic and its host tests; `controller` is the RP2040 I/O around it.

## Stakes record

Draft from the 2026-11 firmware audit; Yan corrects it.

- **Who and how:** Yan alone plays the 123-key 5x25 board over USB into Bitwig on a Mac, where a plugin decodes the comma-bank channels. Up to about 10 keys are held at once. Bitwig may echo notes back to light the LEDs. A 20-key prototype build exists for the bench.
- **Severe:** a stuck, missed, doubled or wrong note; noticeable or variable key-to-MIDI latency; a panic or hang (dead until replugged); a broken 1200-baud flashing reset.
- **Cheap:** a wrong or glitchy LED frame, a dashboard glitch, a lost log line, settings resetting on power cycle (by design).
- **Unsupported, silently dropped:** more than 16 held keys tracked for display, more than 32 notes echoed back, floods of incoming MIDI.
