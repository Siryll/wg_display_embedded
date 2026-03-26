# Architecture

## Overview

WG Display Embedded runs on the dual-core ESP32-S3-Box-3. The firmware has no operating system — it uses the [embassy](https://embassy.dev/) async executor on top of bare-metal `esp-hal` and `esp-rtos`, with a small RTOS layer for task scheduling between the two CPU cores.

Widgets are standalone WebAssembly components compiled against a WIT interface. They run inside an on-device [Wasmtime](https://wasmtime.dev/) runtime and communicate with the host (HTTP, clocks, logging, random) through a set of host functions. Widgets can be added and removed via the UI without having to re-flash.

---

## Dual-Core Execution Model

```
┌──────────────────────────────────────────────────────────────┐
│  Core 0  (PRO_CPU)                                           │
│  ┌────────────────────┐  ┌─────────────────────────────────┐ │
│  │  embassy executor  │  │  HTTP handler task              │ │
│  │  - WiFi tasks      │  │  (async–sync bridge for widgets)│ │
│  │  - net_task        │  └─────────────────────────────────┘ │
│  │  - picoserve HTTP  │                                       │
│  └────────────────────┘                                       │
└──────────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────────┐
│  Core 1  (APP_CPU)                                           │
│  ┌────────────────────────────────────────────────────────┐  │
│  │  Renderer task                                         │  │
│  │  - Runs each widget's WASM component                   │  │
│  │  - Draws widget results to display                     │  │
│  └────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

## Async–Sync HTTP Bridge

Widgets run synchronously inside the Wasmtime runtime on Core 1. The only HTTP client is async (embassy-net / reqwless). To let widgets make HTTP calls without an async runtime on Core 1, a two-channel bridge is used:

```
Core 1 (WASM widget)                Core 0 (embassy async)
─────────────────────               ──────────────────────
widget calls http::request()
  ↓
http_request_sync()
  sends HttpRequest on               HTTP_REQUEST_CHANNEL
  HTTP_REQUEST_CHANNEL  ──────────►  http_handler_task() dequeues
  then polls with 10 ms              executes via EspHttpClient
  RTOS yields (≤ 3000              ◄──  HTTP_RESPONSE_CHANNEL
  iterations = 30 s)                sends HttpResponse
  ↓
returns Response to widget
```

Key properties:
- Channel capacity = 1 (one in-flight request at a time per widget execution)
- Timeout: 30 seconds
- No TLS certificate verification (constraint of the embedded TLS implementation)

Source files: [`runtime/http_sync.rs`](../embedded_app/src/runtime/http_sync.rs), [`http_client/mod.rs`](../embedded_app/src/http_client/mod.rs)

---

## Memory Layout

| Region | Size | Used for |
|---|---|---|
| Internal RAM heap | ~73 KB | Stack frames, small allocations |
| PSRAM | Several MB | Large allocations: framebuffer, HTTP response buffers, WASM runtime |
| Flash — NVS (`0x9000`) | 20 KB | WiFi credentials, system config |
| Flash — Factory app (`0x10000`) | 3 MB | Firmware binary |
| Flash — Storage (`0x310000`) | ~13 MB | Installed widget WASM binaries |

Full partition table: [`embedded_app/partitions.csv`](../embedded_app/partitions.csv)

> **PSRAM init order:** PSRAM must be initialized before the heap allocator because widget HTTP responses (up to 512 KB) and the display framebuffer (153,600 bytes) are allocated in PSRAM. If heap is initialized first, these large allocations will fail.

---
