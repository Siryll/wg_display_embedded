#![allow(dead_code)] // will be used in the future

mod platform;

mod host_api;

use alloc::vec::Vec;
use wasmtime::{Config, Engine, Instance, Module, Store, Result};
use wasmtime::component::Component;
use wasmtime::component::Linker;

// links wit finctions, implementations in host_api
wasmtime::component::bindgen!({ path: "../wg_display/wg_display_widget_wit/wit" });

pub struct WidgetState {
    // Data fields usable in the host API functions
}

impl WidgetState {
    fn new() -> Self {
        Self {}
    }
}


pub struct CompiledModule {
    data: Vec<u8>,
    compatibility_hash: u64,
}

pub struct Runtime {
    engine: Engine,
    linker: Linker<(WidgetState)>,
    store: Store<(WidgetState)>,
    last_run: alloc::collections::BTreeMap<u32, u64>,
}

impl Runtime {
    pub fn new() -> Self {
        defmt::info!("Initializing Wasmtime runtime");
        
        let mut config = Config::new();
        config.wasm_component_model(true);
        
        // disable many optional features: https://github.com/bytecodealliance/wasmtime/blob/main/examples/min-platform/embedding/wasmtime-platform.h
        config.wasm_bulk_memory(false);
        config.wasm_simd(false);
        config.wasm_relaxed_simd(false);
        config.wasm_multi_memory(false);
        config.gc_support(false);

        config.signals_based_traps(false);
        
        config.memory_reservation(0);
        config.memory_guard_size(0);
        config.memory_init_cow(false);
        
        let engine = Engine::new(&config)
            .expect("Failed to create Wasmtime engine");
        
        let linker = Linker::<WidgetState>::new(&engine);

        let store = Store::new(&engine, WidgetState::new());

        Widget::add_to_linker(&mut linker, |state: &mut WidgetState| state)
            .expect("Could not link host API");
        
        defmt::info!("Wasmtime runtime initialized successfully");
        
        Self { engine, linker, store, last_run: alloc::collections::BTreeMap::new()}
    }
    
    pub unsafe fn load_module(&self, bytes: &[u8]) -> Result<Component> {
        defmt::debug!("Loading precompiled module ({} bytes)", bytes.len());
        
        // consideret only safe if compiled on device
        let component = unsafe { Component::deserialize(&self.engine, bytes) }?;
        
        defmt::info!("Module loaded successfully");
        Ok(component)
    }
    
    pub fn instantiate(&mut self, component: &Component) -> Result<Widget> {
        defmt::debug!("Instantiating component");
        
        // let instance = self.linker.instantiate(&mut self.store, module)?;
        let widget = Widget::instantiate(&mut self.store, component, &self.linker)?;
        
        defmt::info!("Component instantiated successfully");
        Ok(widget)
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}