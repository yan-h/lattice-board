MIDI controller using the Wicki-Hayden layout. Heavily inspired by [Hexboard](https://shapingthesilence.com/tech/hexboard-midi-controller/), [Striso](https://www.striso.org) and [Lumatone](https://www.lumatone.io).
- 123 keys with individual LED lighting
  - 5 octaves
  - Fifthspan of 25 - i.e. each octave contains 25 notes, spanning an unbroken segment of the chain of fifths
    - Size of fifth is adjustable in firmware, providing easy access to all temperaments based on pure octaves and a chain of equal-sized perfect fifths.
- Very portable
  - 21x22x2.5cm
  - Similar in size to a small tablet, and much lighter than one.
- No velocity sensitivity (or any other form of MIDI expression). This simplifies the engineering constraints by a lot, and let me design it as a very weirdly shaped mechanical keyboard.

Engineering overview
- Case and keycaps 3D-printed in PETG (Bambu Lab translucent PETG and black PETG-HF)
  - [3D models](https://cad.onshape.com/documents?nodeId=e71b068a262a341693bc0b63&resourceType=folder) made using Onshape. 
  - Printed using a Bambu Lab P1S. Keycaps were printed with the Darkmoon Satin build plate, to obtain a texture finer than a textured PEI plate, but coarser than a smooth plate.
- TTC Frozen Silent V2 Keyswitches - fully transparent, light condenser, silent.
- PCB designed in KiCad 9, manufactured with JLCPCB
- Firmware in Rust with [Embassy](https://embassy.dev/). Heavily vibecoded.

![PXL_20260202_012643973](https://github.com/user-attachments/assets/024bc8af-1104-46bf-8a43-0e63082621a8)
![PXL_20260202_012527108](https://github.com/user-attachments/assets/97ccf371-84ad-441b-bd6c-b121cd6831d7)
