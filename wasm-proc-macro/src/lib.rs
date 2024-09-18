#![allow(internal_features)]
#![feature(proc_macro_internals)]

extern crate proc_macro;
use proc_macro::bridge::buffer::Buffer;
use proc_macro::bridge::client::TokenStream;
use proc_macro::bridge::ExpnGlobals;
use proc_macro::Span;

use std::mem;
use std::os::raw::c_void;
use std::slice::from_raw_parts;

#[no_mangle]
pub extern "C" fn allocate(size: usize) -> *mut c_void {
    let mut buffer = Vec::with_capacity(size);
    let pointer = buffer.as_mut_ptr();
    mem::forget(buffer);

    pointer as *mut c_void
}

#[no_mangle]
pub extern "C" fn deallocate(pointer: *mut c_void, capacity: usize) {
    unsafe {
        let _ = Vec::from_raw_parts(pointer, 0, capacity);
    }
}

#[no_mangle]
pub extern "C" fn run(ptr: *mut c_void, len: usize) -> (*mut c_void, u32) {
    let input_buf: Vec<u8> = unsafe { from_raw_parts(ptr as *mut u8, len).to_vec() };

    let buf = Buffer::from(input_buf);

    let reader = &mut &buf[..];
    let (_globals, input) = <(ExpnGlobals<Span>, TokenStream)>::decode(reader, &mut ());

    let output = make_answer(input);

    let retbuf = Buffer::new();
    Ok::<_, ()>(output).encode(&mut retbuf, &mut ());

    // output.extend(&subject);
    // output.extend(&[b'!']);
    (retbuf.data as *mut c_void, retbuf.len())
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
pub fn make_answer(_input: TokenStream) -> TokenStream {
    "fn answer() -> u8 { 10 }".parse().unwrap()
}
