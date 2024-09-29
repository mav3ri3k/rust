#![allow(internal_features)]
#![feature(proc_macro_internals)]
#![feature(rustc_attrs)]

extern crate proc_macro;

use proc_macro::bridge::client::run_client_buffer;
use proc_macro::{Delimiter, Group, Ident, Literal, Punct, Spacing, TokenStream, TokenTree};

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
    let retbuf = run_client_buffer(make_answer);

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
pub fn make_answer(_input: TokenStream) -> TokenStream {
    let mut tokens = Vec::new();

    tokens.push(TokenTree::Ident(Ident::new("let", proc_macro::Span::call_site())));
    tokens.push(TokenTree::Ident(Ident::new("a", proc_macro::Span::call_site())));
    tokens.push(TokenTree::Punct(Punct::new('=', Spacing::Alone)));
    tokens.push(TokenTree::Literal(Literal::i32_unsuffixed(1)));
    tokens.push(TokenTree::Punct(Punct::new(';', Spacing::Alone)));

    TokenStream::from_iter(tokens)
}
