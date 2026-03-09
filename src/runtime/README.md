# Compiling widgets to for ESP32

- Compile existing widget for the rust `wasm32-unknown-unknown` target

- Turn it into a component with `wasm-tools`:
```bash
wasm-tools component new <input_file> -o <output_file>
```

- Pre-compile the wasm component