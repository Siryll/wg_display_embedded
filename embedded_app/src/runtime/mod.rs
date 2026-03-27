mod platform;

mod host_api;

pub mod http_sync;

use common::models::WidgetInstallationData;
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, Precompiled, Result, Store};

use alloc::string::{String, ToString};

use crate::runtime::widget::widget::types::{Datetime};

use crate::globals;

use hashbrown::HashMap;

use defmt::warn;

// links wit finctions, implementations in host_api
wasmtime::component::bindgen!({ path: "src/runtime/host_api/wit" });

pub struct WidgetState {}

impl WidgetState {
    fn new() -> Self {
        Self {}
    }
}

pub struct Runtime {
    engine: Engine,
    linker: Linker<WidgetState>,
    last_run: HashMap<String, Datetime>,
}

impl Runtime {
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
    unsafe fn load_module(&self, bytes: &[u8]) -> Result<Component> {
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
    fn instantiate(
        &mut self,
        component: &Component,
        store: &mut Store<WidgetState>,
    ) -> Result<Widget> {
        defmt::debug!("Instantiating component");

        let widget = match Widget::instantiate(store, component, &self.linker) {
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
    fn run(
        &mut self,
        widget: &Widget,
        config: String,
        store: &mut Store<WidgetState>,
        name: String,
    ) -> wasmtime::Result<Option<WidgetResult>> {
        defmt::info!("Running widget with config: {}", config.as_str());
        let last_invocation = *self.last_run.get(name.as_str())
        .unwrap_or(
             &globals::now()
                .unwrap_or(Datetime {
                seconds: 0,
                nanoseconds: 0,
        }));

        let context = WidgetContext {
            last_invocation,
            config,
        };

        let result = match widget.call_run(store, &context) {
            Ok(result) => result,
            Err(err) => {
                defmt::error!("Failed to run widget: {:?}", defmt::Debug2Format(&err));
                return Err(err);
            }
        };

        self.last_run.insert(
            name,
            globals::now()
                .unwrap_or(Datetime {
                seconds: 0,
                nanoseconds: 0,
            }),
        );

        defmt::info!("Widget ran successfully result: {}", result.data.as_str());
        Ok(Some(result))
    }

    /// Returns the widget's display name (calls `get-name` WIT export).
    fn get_widget_name(
        &mut self,
        widget: &Widget,
        store: &mut Store<WidgetState>,
    ) -> wasmtime::Result<String> {
        widget.call_get_name(store)
    }

    /// Returns the widget's JSON Schema config string (calls `get-config-schema` WIT export).
    fn get_config_schema(
        &mut self,
        widget: &Widget,
        store: &mut Store<WidgetState>,
    ) -> wasmtime::Result<String> {
        widget.call_get_config_schema(store)
    }

    /// Returns the widget's semver version string (calls `get-version` WIT export).
    fn get_widget_version(
        &mut self,
        widget: &Widget,
        store: &mut Store<WidgetState>,
    ) -> wasmtime::Result<String> {
        widget.call_get_version(store)
    }

    /// Returns how often the widget should be run in seconds (calls `get-run-update-cycle-seconds`).
    fn get_run_update_cycle_seconds(
        &mut self,
        widget: &Widget,
        store: &mut Store<WidgetState>,
    ) -> wasmtime::Result<u32> {
        widget.call_get_run_update_cycle_seconds(store)
    }

    /// Wrapper function for running a widget by name with given json config
    pub async unsafe fn run_widget(&mut self, widget_name: String, config: String) -> wasmtime::Result<Option<WidgetResult>> {
        let mut store = Store::new(&self.engine, WidgetState::new());

        let wasm_bytes = match globals::with_storage(|s| s.wasm_read(&widget_name)).await {
            Ok(bytes) => bytes,
            Err(err) => {
                warn!(
                    "Could not read widget '{}': {:?}",
                    widget_name.as_str(),
                    defmt::Debug2Format(&err)
                );
                return Err(wasmtime::Error::msg("Widget binary missing"));
            }
        };

        let component = unsafe { self.load_module(&wasm_bytes)? };
        let mut instance = self.instantiate(&component, &mut store)?;
        self.run(&mut instance, config, &mut store, widget_name)
    }

    /// wrapper function to get all widget metadata with the same store
    pub async unsafe fn get_widget_metadata(&mut self, bytes: &[u8]) -> wasmtime::Result<WidgetInstallationData> {
        let mut store = Store::new(&self.engine, WidgetState::new());
        let component = unsafe { self.load_module(&bytes)? };
        let instance = self.instantiate(&component, &mut store)?;
        let name = self.get_widget_name(&instance, &mut store)?;
        let json_config_schema = self.get_config_schema(&instance, &mut store)?;
        let version = self.get_widget_version(&instance, &mut store)?;
        let update_cycle_seconds = self.get_run_update_cycle_seconds(&instance, &mut store)?;

        Ok(WidgetInstallationData {
            name,
            description: String::new(), // description is not currently stored in the component, could be added as a custom section if needed
            version,
            json_config: "{}".to_string(),
            json_config_schema,
            update_cycle_seconds,
        })
    }
}
