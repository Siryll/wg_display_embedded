# Frontend

A [Yew](https://yew.rs/) (Rust/WASM) single-page application served from the ESP32.

## Technology

| Library | Purpose |
|---|---|
| Yew 0.20 | Rust/WASM UI framework |
| Trunk | WASM build tool (compiles + bundles) |
| Tailwind CSS | Utility-first CSS |
| Bootstrap | CSS base + grid |
| jsonform | Auto-generates config forms from JSON Schema |


## Pages

| Route | Page | Description |
|---|---|---|
| `/` | Home | Lists installed widgets, per-widget config access |
| `/install` | Install | Browse widget store, install from URL |
| `/config/<name>` | Config | Edit per-widget config (form generated from JSON Schema) |

## Build

Install dependencies (first time only):
```bash
npm install
rustup target add wasm32-unknown-unknown
```

Build Tailwind CSS:
```bash
npm run tailwind-build
```

Build the WASM frontend:
```bash
trunk build --release
```

Output goes to `dist/`. The main embedded app build (`make build`) runs both of these automatically.