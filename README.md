# rustyNES

RustyNES - Nintendo Entertainment System (NES) Emulator (Rust Language)

![License](https://img.shields.io/github/license/doublegate/rustyNES)
![Version](https://img.shields.io/badge/version-0.1.2-blue)

## Features

- Cycle-accurate CPU emulation (MOS 6502 / Ricoh 2A03)
- Complete instruction set implementation (including unofficial opcodes)
- Accurate timing and interrupt handling

## Roadmap

- [ ] PPU (Picture Processing Unit) implementation
- [ ] APU (Audio Processing Unit) implementation
- [ ] Controller input
- [ ] Memory mappers support
- [ ] GUI with SDL2
- [ ] Save states
- [ ] Debugger

## Building

### Prerequisites

- Rust 1.65 or later
- Cargo

### Build Instructions

```bash
# Clone the repository
git clone https://github.com/yourusername/rustyNES.git
cd rustyNES
```

```bash
# Build in release mode
cargo build --release
```

```bash
# Run the emulator
cargo run --release -- /path/to/your/rom.nes
```

## Usage

```bash
rustyNES [OPTIONS] <ROM_PATH>

Arguments:
  <ROM_PATH>  Path to the NES ROM file

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## Project Structure

```bash
src/
├── main.rs           # Entrypoint
├── nes_cpu.rs        # CPU implementation (6502/2A03)
├── ppu/              # PPU implementation
├── apu/              # APU implementation
├── mappers/          # Memory mapper implementations
└── io/               # Controllers and input devices
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
Please read CONTRIBUTING.md for details on our code of conduct and the process for submitting pull requests.

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Acknowledgments

- NESdev Wiki for their excellent documentation on NES hardware
- The various test ROMs created by the community to verify emulator accuracy
