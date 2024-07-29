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
pub fn make_answer() -> u32 {
    println!("Wasm proc-macro is being run");
    42
}
