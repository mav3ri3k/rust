#![allow(internal_features)]
#![feature(proc_macro_internals)]
use pm::*;

use std::mem::size_of_val;
use wasmtime::*;

fn load_server() {
    let ts: pm::TokenStream = "fn ans() -> { let x = 10; let y = 10; x + y }".parse().unwrap();

    let engine = Engine::default();

    // Load the WebAssembly module from a file
    let module = Module::from_file(
        &engine,
        "../pass_wasm/target/wasm32-unknown-unknown/release/pass_wasm.wasm",
    )
    .unwrap();

    // Create a store instance
    let mut store = Store::<()>::default();

    let instance = Instance::new(&mut store, &module, &[]).unwrap();

    // FIXME(mav3ri3k) The size can not be increased
    let alloc = instance.get_typed_func::<i32, i32>(&mut store, "alloc").unwrap();

    let _memory = instance.get_memory(&mut store, "memory").unwrap();
    let size = size_of_val(&ts) as i32;

    // Call the exported function with the input data
    let output_data = alloc.call(&mut store, size).unwrap();

    println!("Output: {:?}", output_data);
}
