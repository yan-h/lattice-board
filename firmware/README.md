# LatticeBoard firmware

Rust firmware for the RP2040, built on [Embassy](https://embassy.dev/). The board enumerates as a composite USB device with two interfaces:

- **USB-MIDI** — the actual instrument output
- **USB-CDC serial** — live logs and an interactive configuration dashboard (see below)

## Workspace layout

- `core/` — hardware-independent layout and pitch math (`layout.rs`, `pitch.rs`)
- `controller/` — the firmware binary: key scanning, LEDs, MIDI/MPE, tuning, USB

## Hardware layouts (cargo features)

Two mutually exclusive features in `controller/Cargo.toml` select the target hardware:

| Feature | Hardware | LEDs | LED data pin |
|---|---|---|---|
| `layout-prototype` (**default**) | small test board | 20 | GPIO 29 |
| `layout-5x25` | the full 123-key board | 125 | GPIO 3 |

The default is the prototype, so **building the real keyboard requires overriding the default feature** (see below).

## Prerequisites

- [rustup](https://rustup.rs) — `rust-toolchain.toml` pins Rust 1.90 and the `thumbv6m-none-eabi` target; both are installed automatically the first time you run cargo in this directory.
- [`elf2uf2-rs`](https://crates.io/crates/elf2uf2-rs) — used by the cargo runner to flash: `cargo install elf2uf2-rs`

## Build

```sh
cd firmware
cargo build --release --no-default-features --features layout-5x25   # full board
cargo build --release                                                # prototype
```

## Flash

Flashing is wired into `cargo run` via `.cargo/config.toml` — no debug probe needed, just USB:

```sh
cargo run --release --no-default-features --features layout-5x25
```

The runner script:

1. Sets the board's CDC serial port (`/dev/tty.usbmodem*`) to 1200 baud. The firmware treats a 1200-baud line coding as a "reboot into the UF2 bootloader" signal (`check_for_reset` in `controller/src/usb.rs`).
2. Waits 3 seconds for the `RPI-RP2` mass-storage bootloader to mount.
3. Runs `elf2uf2-rs -d`, which converts the ELF to UF2 and copies it to the board.

**Recovery / first flash:** if the board isn't running this firmware (so the 1200-baud trick doesn't work), hold the RP2040's BOOTSEL button while plugging in USB, then run the same `cargo run` command — the `stty` step fails harmlessly and `elf2uf2-rs` deploys as usual.

The runner script is macOS-specific (`stty -f /dev/tty.usbmodem*`); on Linux adjust to `stty -F /dev/ttyACM*`.

## Serial dashboard

Connect any terminal emulator to the CDC port (any baud rate except 1200, which triggers the bootloader reset):

```sh
screen /dev/tty.usbmodem*
```

The port starts in **log mode**, streaming `log`/defmt-style output. Press `d` to toggle the live **dashboard**, which shows brightness, hue, tuning mode, fifth size, MPE pitch-bend range, held keys, and remote MIDI voices, plus a key legend:

| Keys | Action |
|---|---|
| `[` / `]` | select previous/next of the 12 note-color anchors |
| `r`/`R`, `g`/`G`, `b`/`B` | adjust the selected anchor's RGB channels ±5 |
| `H` / `h` | shift hue offset ±1° |
| `L` / `l` | brightness ±0.05 |
| `+` / `-` | brightness ±0.02 (fine) |
| `t` | toggle tuning mode: `Standard` (12-TET MIDI notes) ↔ `Fifths` (MPE, adjustable fifth) |
| `(` / `)` | fifth size ±1 cent (clamped to 600–800¢) |
| `{` / `}` | fifth size ±0.1 cent |
| `d` | back to log mode |

Settings are in-memory only — they reset to the defaults in `controller/src/leds.rs` and `controller/src/tuning.rs` on power cycle. To persist a tweak, change the defaults there and reflash.
