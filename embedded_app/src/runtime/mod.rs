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

pub struct WidgetState {}

impl WidgetState {
    fn new() -> Self {
        Self {}
    }
}

#[allow(dead_code)]
pub struct CompiledModule {
    data: Vec<u8>,
    compatibility_hash: u64,
}

pub struct Runtime {
    engine: Engine,
    linker: Linker<WidgetState>,
    store: Store<WidgetState>,
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

    pub fn get_widget_name(&mut self, widget: &Widget) -> wasmtime::Result<String> {
        widget.call_get_name(&mut self.store)
    }

    pub fn get_config_schema(&mut self, widget: &Widget) -> wasmtime::Result<String> {
        widget.call_get_config_schema(&mut self.store)
    }

    pub fn get_widget_version(&mut self, widget: &Widget) -> wasmtime::Result<String> {
        widget.call_get_version(&mut self.store)
    }

    pub fn get_run_update_cycle_seconds(&mut self, widget: &Widget) -> wasmtime::Result<u32> {
        widget.call_get_run_update_cycle_seconds(&mut self.store)
    }
}
