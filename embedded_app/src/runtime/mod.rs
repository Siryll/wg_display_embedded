//! Wasmtime WebAssembly Component Model runtime for widget execution.
//!
//! Widgets are precompiled WASM components that export a standard WIT interface.
//! Each widget execution creates a fresh [`Runtime`] instance — no WASM state
//! persists between runs.
//!
//! ## Wasmtime feature flags
//! Many optional Wasmtime features are disabled to reduce binary size and
//! memory usage on the ESP32-S3:
//! - SIMD, multi-value, tail-call, GC, concurrency: **disabled**
//! - signals-based traps, memory reservation/guard, copy-on-write: **disabled**
//! - Component model + bulk-memory: **enabled** (required for WIT)
//!
//! ## Module Files
//!
//! | File | Purpose |
//! |---|---|
//! | `mod.rs` | [`Runtime`] struct — engine, linker, store, widget lifecycle |
//! | `platform.rs` | Per-core thread-local storage for Wasmtime (PRO_CPU = 0, APP_CPU = 1) |
//! | `http_sync.rs` | Async-to-sync HTTP bridge for widget HTTP calls |
//! | `host_api/` | Host function implementations (http, clocks, logging, random) |
//!
//! ## Precompiled Widgets
//!
//! [`Runtime::load_module`] expects a **precompiled** WASM component (`Component::deserialize`),
//! not a raw `.wasm` binary. Raw components must be precompiled off-device using a Wasmtime
//! build targeting `xtensa-esp32s3-none-elf` with the same Wasmtime version (42.0.1).
//! See `docs/widget-development.md` for the full build pipeline.
//!
//! ## HTTP Bridge Consumers
//!
//! [`http_sync::http_request_sync`] is called from both async and sync contexts:
//!
//! | Consumer | Purpose |
//! |---|---|
//! | Host API `http.rs` | widget WASM HTTP calls (sync side of bridge) |
//! | `widget/store/mod.rs` | fetch remote `widget_store.json` |
//! | `widget/manager/mod.rs` | download widget WASM binary for installation |
//! | `util/esptime.rs` | fetch Unix timestamp from `timeapi.io` |
//!
//! ## Host API (`host_api/`)
//!
//! | Module | WIT interface | Implementation |
//! |---|---|---|
//! | `http.rs` | `http` | calls [`http_sync::http_request_sync`] |
//! | `clocks.rs` | `clocks` | reads [`globals::now_parts`](crate::util::globals::now_parts) |
//! | `logging.rs` | `logging` | maps to `defmt::{debug,info,warn,error}` with `[WIDGET]` prefix |
//! | `random.rs` | `random` | two reads from ESP32 hardware RNG → `u64` |
//!
//! WIT definitions: `embedded_app/src/runtime/host_api/wit/*.wit`
mod platform;

mod host_api;

pub mod http_sync;

use alloc::vec::Vec;
use hashbrown::HashMap;
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, Precompiled, Result, Store};

use alloc::string::String;

use crate::runtime::widget::widget::types::Datetime;

// links wit finctions, implementations in host_api
wasmtime::component::bindgen!({ path: "src/runtime/host_api/wit" });

/// Marker type for Wasmtime host state. No host state is stored per-instance.
pub struct WidgetState {}

impl WidgetState {
    fn new() -> Self {
        Self {}
    }
}

/// A precompiled WASM component binary with its Wasmtime compatibility hash.
#[allow(dead_code)]
pub struct CompiledModule {
    data: Vec<u8>,
    compatibility_hash: u64,
}

/// Wasmtime runtime handle for loading, instantiating, and executing widget components.
pub struct Runtime {
    engine: Engine,
    linker: Linker<WidgetState>,
    store: Store<WidgetState>,
    last_run: HashMap<String, Datetime>,
}

impl Runtime {
    /// Creates a new runtime with an optimised Wasmtime configuration for embedded use.
    pub fn new() -> Self {
        defmt::info!("Initializing Wasmtime runtime");

        let mut config = Config::new();
        config.wasm_component_model(true);

        // disable many optional features: https://github.com/bytecodealliance/wasmtime/blob/main/examples/min-platform/embedding/wasmtime-platform.h
        config.wasm_bulk_memory(true);
        config.wasm_simd(false);
        config.wasm_relaxed_simd(false);
        config.wasm_multi_memory(false);
        config.gc_support(false);

        config.signals_based_traps(false);
        // config.wasm_multi_value(true);
        config.wasm_multi_value(false);
        // config.wasm_tail_call(true);
        config.wasm_tail_call(false);

        config.memory_reservation(0);
        // config.memory_reservation(0);
        config.memory_guard_size(0);
        config.memory_init_cow(false);
        config.concurrency_support(false);

        let engine = Engine::new(&config).expect("Failed to create Wasmtime engine");

        let store = Store::new(&engine, WidgetState::new());

        let mut linker = Linker::<WidgetState>::new(&engine);
        // Use the HasSelf wrapper type for component model
        Widget::add_to_linker::<WidgetState, HasSelf<WidgetState>>(
            &mut linker,
            |state: &mut WidgetState| state,
        )
        .expect("Could not link host API");

        defmt::info!("Wasmtime runtime initialized successfully");

        Self {
            engine,
            linker,
            store,
            last_run: HashMap::new(),
        }
    }

    /// Deserialises a precompiled Wasmtime component from raw bytes.
    ///
    /// # Safety
    /// The bytes **must** be a Wasmtime precompiled component artifact produced
    /// by the same Wasmtime version (42.0.1) targeting `xtensa-esp32s3-none-elf`.
    /// Passing a raw `.wasm` file or a mismatched artifact will return an error.
    ///
    /// # Errors
    /// Returns an error if `bytes` is not recognised as a precompiled component.
    pub unsafe fn load_module(&self, bytes: &[u8]) -> Result<Component> {
        defmt::debug!("Loading precompiled module ({} bytes)", bytes.len());

        match Engine::detect_precompiled(bytes) {
            Some(Precompiled::Component) => {}
            Some(Precompiled::Module) => {
                defmt::error!("Precompiled blob is a core module, but runtime expects a component");
                return Err(wasmtime::Error::msg("expected precompiled component"));
            }
            None => {
                defmt::error!("Input bytes are not recognized as a Wasmtime precompiled artifact");
                return Err(wasmtime::Error::msg("invalid precompiled artifact"));
            }
        }

        // consideret only safe if compiled on device
        let component = match unsafe { Component::deserialize(&self.engine, bytes) } {
            Ok(component) => component,
            Err(err) => {
                defmt::error!(
                    "Failed to deserialize component: {:?}",
                    defmt::Debug2Format(&err)
                );
                return Err(err);
            }
        };

        defmt::info!("Module loaded successfully");
        Ok(component)
    }

    /// Binds host functions and instantiates a loaded component.
    ///
    /// # Errors
    /// Returns an error if the component's imports cannot be satisfied by the
    /// current host API linker (e.g. WIT interface mismatch).
    pub fn instantiate(&mut self, component: &Component) -> Result<Widget> {
        defmt::debug!("Instantiating component");

        let widget = match Widget::instantiate(&mut self.store, component, &self.linker) {
            Ok(widget) => widget,
            Err(err) => {
                defmt::error!(
                    "Failed to instantiate component: {:?}",
                    defmt::Debug2Format(&err)
                );
                return Err(err);
            }
        };

        defmt::info!("Component instantiated successfully");
        Ok(widget)
    }

    /// Calls the widget's `run` export with the given JSON config string.
    ///
    /// Passes a [`WidgetContext`] containing the last-invocation timestamp and
    /// the widget's current config. Returns the [`WidgetResult`] containing the
    /// text to display on screen.
    pub fn run(
        &mut self,
        widget: &Widget,
        config: String,
    ) -> wasmtime::Result<Option<WidgetResult>> {
        defmt::info!("Running widget with config: {}", config.as_str());
        let name = self.get_widget_name(widget)?;
        let last_invocation = *self.last_run.get(name.as_str()).unwrap_or(&Datetime {
            seconds: 0,
            nanoseconds: 0,
        });

        let context = WidgetContext {
            last_invocation,
            config,
        };

        let result = match widget.call_run(&mut self.store, &context) {
            Ok(result) => result,
            Err(err) => {
                defmt::error!("Failed to run widget: {:?}", defmt::Debug2Format(&err));
                return Err(err);
            }
        };

        // TODO: fix time handling
        self.last_run.insert(
            name,
            Datetime {
                seconds: 0,
                nanoseconds: 0,
            },
        );

        defmt::info!("Widget ran successfully result: {}", result.data.as_str());
        Ok(Some(result))
    }

    /// Returns the widget's display name (calls `get-name` WIT export).
    pub fn get_widget_name(&mut self, widget: &Widget) -> wasmtime::Result<String> {
        widget.call_get_name(&mut self.store)
    }

    /// Returns the widget's JSON Schema config string (calls `get-config-schema` WIT export).
    pub fn get_config_schema(&mut self, widget: &Widget) -> wasmtime::Result<String> {
        widget.call_get_config_schema(&mut self.store)
    }

    /// Returns the widget's semver version string (calls `get-version` WIT export).
    pub fn get_widget_version(&mut self, widget: &Widget) -> wasmtime::Result<String> {
        widget.call_get_version(&mut self.store)
    }

    /// Returns how often the widget should be run in seconds (calls `get-run-update-cycle-seconds`).
    pub fn get_run_update_cycle_seconds(&mut self, widget: &Widget) -> wasmtime::Result<u32> {
        widget.call_get_run_update_cycle_seconds(&mut self.store)
    }
}
