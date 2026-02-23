use core::ptr;

// most basic func needed to run wasmtime in no_std: https://github.com/bytecodealliance/wasmtime/blob/main/examples/min-platform/embedding/wasmtime-platform.h

static mut WASMTIME_TLS: *mut u8 = ptr::null_mut();


#[unsafe(no_mangle)]
pub unsafe extern "C" fn wasmtime_tls_get() -> *mut u8 {
    unsafe { WASMTIME_TLS }
}


#[unsafe(no_mangle)]
pub unsafe extern "C" fn wasmtime_tls_set(ptr: *mut u8) {
    unsafe { WASMTIME_TLS = ptr; }
}
