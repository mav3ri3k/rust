//! This module implements: wasm procedural macros
//! "wpm": Get it ?

#![allow(unused)]
mod client;
mod server;

/*
fn encode(ts: pm::TokenStream) -> Vec<u8> {
    ts.to_string().into_bytes()
}

fn decode(bytes: Vec<u8>) -> pm::TokenStream {
    String::from_utf8(bytes).unwrap().as_str().parse().unwrap()
}

*/
