//! Client-wasm module communication.
#![allow(unused)]

use wain_exec;
use wain_syntax_binary;
use wain_validate;

use wain_exec::execute;
use wain_exec::{DefaultImporter, Runtime, Value};
use wain_syntax_binary::parse;
use wain_validate::validate;

use super::client::TokenStream;

use std::fs;
use std::io;
use std::mem;
use std::os::raw::c_void;
use std::process::exit;
use std::ptr;

fn instantiate_wpm(path: &str) -> Option<Runtime> {
    let source = fs::read(path).unwrap();

    // Parse binary into syntax tree
    let tree = match parse(&source) {
        Ok(tree) => tree,
        Err(err) => {
            eprintln!("Could not parse: {}", err);
            return None;
        }
    };

    // Validate module
    if let Err(err) = validate(&tree) {
        eprintln!("This .wasm file is invalid!: {}", err);
        return None;
    }

    // Create default importer to call external function supported by default
    let stdin = io::stdin();
    let stdout = io::stdout();
    let importer = DefaultImporter::with_stdio(stdin.lock(), stdout.lock());

    // Make abstract machine runtime. It instantiates a module
    let mut runtime = match Runtime::instantiate(&tree.module, importer) {
        Ok(m) => m,
        Err(err) => {
            eprintln!("could not instantiate module: {}", err);
            return None;
        }
    };

    runtime
}

//TODO(mav3ri3k) P1
//I am doing some major ownership mishap here
//This should be all red,
//maybe compiler not complaining because of unsafe ?
pub(super) fn eval_wpm(input: TokenStream, path: Path) -> Option<TokenStream> {
    // Read wasm binary
    let mut runtime = instantiate_wpm(path)?;
    //TODO(mav3ri3k)
    //Works Only if you are on a 32-bit machine. Otherwise,
    //the cast of the pointer to an i32 will truncate,
    //and you won't be able to get the original address back.
    //Switching to use an isize to handle pointer-sized integers instead,
    //you can cast the pointer back to the HashMap type, dereference it,
    //then borrow that instance of the hash map:
    //form good folks at stackoverflow
    //
    // Check this
    let ts_ptr = ptr::addr_of!(input) as i32;

    // Let's say `int add(int, int)` is exported
    //INFO(mav3ri3k) run_wpm takes pointer for input TokenStream
    //Returns pointer for output tokenstream
    match runtime.invoke("run_wpm", &[Value::I32(ts_ptr)]) {
        Ok(ret) => {
            // `ret` is type of `Option<Value>` where it contains `Some` value when the invoked
            // function returned a value. Otherwise it's `None`.
            if let Some(Value::I32(ptr)) = ret {
                let ts_ptr: *mut TokenStream = unsafe { ptr as *mut TokenStream };
                let ts: &mut TokenStream = unsafe { &mut *ts_ptr };

                ts
            } else {
                None
                //TODO(mav3ri3k)
                // Why did pattern matching not work for unreachable!() ?
                //unreachable!();
            }
        }
        // None is proxy for error
        Err(trap) => None,
    }
}
