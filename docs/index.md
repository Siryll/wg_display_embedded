---
layout: default
---

<script type="module" src="https://unpkg.com/esp-web-tools@10/dist/web/install-button.js?module"></script>

## ⚡ One-Click Install

Flash the latest firmware directly from your browser — no drivers or toolchain required.

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

## Manual Install

Download the latest `firmware.bin` from [GitHub Pages](https://siryll.github.io/wg_display_embedded/firmware.bin) and flash with:

```
espflash flash --chip esp32s3 firmware.bin
```

---

## Rust Documentation

- [common crate rustdoc](rustdoc/common/common/index.html) — Shared models and types
- [embedded_app rustdoc](rustdoc/embedded_app/embedded_wg_display/index.html) — Firmware and runtime
- [frontend rustdoc](rustdoc/frontend/frontend/index.html) — Yew web UI components

---

[View source on GitHub](https://github.com/Siryll/wg_display_embedded)
