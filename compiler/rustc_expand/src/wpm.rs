use std::path::PathBuf
use wamstime::*;
extern crate proc_macro;
use proc_macro::TokenStream;

struct host {}

struct remote {}

fn token_stream_to_bytes(stream: TokenStream) -> Vec<u8> {
    bincode::serialize(&stream).umwrap()
} 

fn communicate(path: PathBuf) -> Result<()>{
    let engine = Engine::default();
    // this module only imports and exports i32 for bytes communication
    // TODO change this from aboslute path
    let module = Module::from_file(&engine, PathBuf)?;

    let mut linker = Linker::new(&engine);
    linker.func_wrap("host", "get_byte", |x: u32| -> u8 {
    linker.func_wrap("host", "get_size", || -> u32 {
    })?;
}
