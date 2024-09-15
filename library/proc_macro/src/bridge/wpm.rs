#![allow(dead_code)]
use crate::TokenStream;

mod data;
mod decode;
mod encode;
mod runtime;

//pub fn proc_macro(fun: &str, inputs: Vec<TokenStream>, instance: &WasmMacro) -> TokenStream {

pub(super) fn eval_wpm(_input: TokenStream) -> TokenStream {
    todo!()
}
