# WG Display Embedded!

Port of the [wg_display](https://github.com/eliabieri/wg_display) running on the ESP32-S3-Box-3.

## Requirements
- [rustup](https://rustup.rs/) installed
- [espup](https://docs.espressif.com/projects/rust/book/getting-started/toolchain.html#xtensa-devices) installed
- The [ESP32-S3-Box-3](https://github.com/espressif/esp-box/blob/master/docs/hardware_overview/esp32_s3_box_3/hardware_overview_for_box_3.md)

With this current build there seems to be an issue when building under Windows, try to build in WSL by attaching the serial port connected to the ESP32-S3-Box-3 according to the [WSL instructions for ESP](https://docs.espressif.com/projects/vscode-esp-idf-extension/en/latest/additionalfeatures/wsl.html#adding-the-required-linux-packages-in-wsl)

## Installation
Clone the repo:
```bash
git clone https://github.com/Siryll/wg_display_embedded.git
```

Build the project:
```bash
cargo build --release
```
