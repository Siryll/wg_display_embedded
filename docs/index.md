---
layout: default
---

<script type="module" src="https://unpkg.com/esp-web-tools@10/dist/web/install-button.js?module"></script>

## ⚡ One-Click Install

Flash the latest firmware directly from your browser:

<esp-web-install-button manifest="manifest.json">
  <button slot="activate">Install / Update Firmware</button>
</esp-web-install-button>

> **Supported browsers:** Chrome or Edge on desktop (requires Web Serial API).

---

## Requirements

- ESP32-S3-Box-3 connected via USB
- A USB cable that supports data transfer
- Chrome or Edge browser (desktop)

---

## Setup

After installation connect to the `Wg Display AP` WiFi network with the password `wgdisplay123`, open your browser and navigate to [http://192.168.2.1].
Set the SSID and password of your local home wifi and press `Save and reboot`, the device will reboot and connect to your local network.
The device address will be visible on the display, opening this address in your browser will allow you to configure and install widgets.

### Install widgets

In the Web UI of the WG-Display, press on the `+` button at the bottom to bring up the installation screen. 
Here widgets can be installed from the store or via a direct URL pointing to a widget created with the [wg_display_embedded_widget_template](https://github.com/Siryll/wg_display_embedded_widget_template).

### Configure widgets

From the Web UI home page each installed widget can be configured, some widget will require this to work otherwise they will produce no output.

---

## Manual Install

Download the latest `firmware.bin` from [GitHub Pages](https://siryll.github.io/wg_display_embedded/firmware.bin) and flash with:

```
espflash flash --chip esp32s3 firmware.bin
```

---

## Creating own widgets

Take a look at the [wg_display_embedded_widget_template](https://github.com/Siryll/wg_display_embedded_widget_template) to find out how to creat your own widget.

---

## Rust Documentation

- [common crate rustdoc](rustdoc/common/common/index.html) — Shared models and types
- [embedded_app rustdoc](rustdoc/embedded_app/embedded_wg_display/index.html) — Firmware and runtime
- [frontend rustdoc](rustdoc/frontend/frontend/index.html) — Yew web UI components

---

[View source on GitHub](https://github.com/Siryll/wg_display_embedded)
