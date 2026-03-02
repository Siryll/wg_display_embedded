use core::ptr;
// use core::sync::atomic::{AtomicPtr, Ordering};
// use esp_hal::system::Cpu;

// most basic func needed to run wasmtime in no_std: https://github.com/bytecodealliance/wasmtime/blob/main/examples/min-platform/embedding/wasmtime-platform.h

// potential safe alternative, tough a bit more complex:

// static WASMTIME_TLS: [AtomicPtr<u8>; 2] = [
//     AtomicPtr::new(ptr::null_mut()),
//     AtomicPtr::new(ptr::null_mut()),
// ];

// #[inline(always)]
// fn current_core_index() -> usize {
//     match Cpu::current() {
//         Cpu::ProCpu => 0,
//         _ => 1,
//     }
// }


// #[unsafe(no_mangle)]
// pub extern "C" fn wasmtime_tls_get() -> *mut u8 {
//     WASMTIME_TLS[current_core_index()].load(Ordering::SeqCst)
// }


// #[unsafe(no_mangle)]
// pub extern "C" fn wasmtime_tls_set(ptr: *mut u8) {
//     WASMTIME_TLS[current_core_index()].store(ptr, Ordering::SeqCst);
// }

static mut WASMTIME_TLS: *mut u8 = ptr::null_mut();


#[unsafe(no_mangle)]
pub unsafe extern "C" fn wasmtime_tls_get() -> *mut u8 {
    unsafe { WASMTIME_TLS }
}


#[unsafe(no_mangle)]
pub unsafe extern "C" fn wasmtime_tls_set(ptr: *mut u8) {
    unsafe { WASMTIME_TLS = ptr; }
}
