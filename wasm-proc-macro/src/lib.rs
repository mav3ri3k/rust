#![allow(internal_features)]
#![feature(proc_macro_internals)]

extern crate proc_macro;

use proc_macro::bridge::client::run_client_buffer;
use proc_macro::TokenStream;

use std::mem;
use std::os::raw::c_void;
use std::ptr::copy;
use std::slice::from_raw_parts;

#[no_mangle]
pub extern "C" fn allocate(size: usize) -> *mut c_void {
    println!("In allocate function");
    let mut buffer = Vec::with_capacity(size);
    let pointer = buffer.as_mut_ptr();
    mem::forget(buffer);

    println!("Space allocated!");
    pointer as *mut c_void
}

#[no_mangle]
pub extern "C" fn deallocate(pointer: *mut c_void, capacity: usize) {
    unsafe {
        let _ = Vec::from_raw_parts(pointer, 0, capacity);
    }
}

#[no_mangle]
pub extern "C" fn new_macro(ptr: u32, len: u32) -> u32 {
    ptr + len
}

#[no_mangle]
pub extern "C" fn run_macro(ptr: *mut c_void, len: usize) -> u32 {
    let input_buf: Vec<u8> = unsafe { from_raw_parts(ptr as *mut u8, len).to_vec() };
    let retbuf = run_client_buffer(input_buf.into(), make_answer);

    unsafe {
        copy(retbuf.data as *mut c_void, ptr, retbuf.len());
    }

    retbuf.len().try_into().unwrap()
}

static MACRO_NAME: &str = "make_answer";

#[no_mangle]
pub extern "C" fn macro_name() -> *const u8 {
    println!("Wasm proc-macro has been loaded");
    MACRO_NAME.as_ptr()
}

#[no_mangle]
pub fn macro_name_len() -> usize {
    MACRO_NAME.len()
}

#[no_mangle]
pub fn make_answer() -> TokenStream {
    "fn answer() -> u8 { 10 }".parse().unwrap()
}
