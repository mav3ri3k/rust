//use wasm_bindgen::prelude::*;
extern crate proc_macro as pm;

use std::mem;
use std::os::raw::c_void;
/*
use std::ptr::copy;
use std::slice;
use std::str;
*/

#[no_mangle]
pub extern "C" fn alloc(size: usize) -> *mut c_void {
    let mut buf = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    mem::forget(buf);
    return ptr as *mut c_void;
}

#[no_mangle]
pub extern "C" fn run(ptr: *mut c_void) {
    let ts_ptr: *mut pm::TokenStream = unsafe { ptr as *mut pm::TokenStream };
    let ts: &mut pm::TokenStream = unsafe { &mut *ts_ptr };

    test(ts);
    //TODO(mav3ri3k) Dump this into memory again
}

fn test(_ts: &pm::TokenStream) -> pm::TokenStream {
    "let x = 12;".parse().unwrap()
}
