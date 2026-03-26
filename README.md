# WG Display Embedded!

Port of the [wg_display](https://github.com/eliabieri/wg_display) running on the ESP32-S3-Box-3.

## Install

Install the latest release directly to the ESP32-S3-Box-3 from [here](https://siryll.github.io/wg_display_embedded/).

## Building from source
### Requirements
- [rustup](https://rustup.rs/)
- [espup](https://docs.espressif.com/projects/rust/book/getting-started/toolchain.html#xtensa-devices)
- [NPM/Node](https://docs.npmjs.com/downloading-and-installing-node-js-and-npm)
- The [ESP32-S3-Box-3](https://github.com/espressif/esp-box/blob/master/docs/hardware_overview/esp32_s3_box_3/hardware_overview_for_box_3.md)

When using WSL follow the [WSL instructions for ESP](https://docs.espressif.com/projects/vscode-esp-idf-extension/en/latest/additionalfeatures/wsl.html#adding-the-required-linux-packages-in-wsl) instructions for the correct setup.

### Installation
For `picoserve` the 1.93.0 pre-release version of the espup rust compiled is needed, install it with:
```bash
espup install --toolchain-version 1.93.0
```

The frontend requires [Trunk](https://crates.io/crates/trunk) to build:
```bash
cargo install trunk
```

### Build
Clone the repo:
```bash
git clone https://github.com/Siryll/wg_display_embedded.git
```

Build the project:
```bash
make build
```

Flash to ESP:
```bash
make flash
```
