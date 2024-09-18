#![allow(internal_features)]
#![feature(proc_macro_internals)]

use anyhow::Context;
use anyhow::Result;
use proc_macro::bridge::client::ProcMacro;
use proc_macro::bridge::buffer::Buffer;
use proc_macro::bridge::client::WasmProcMacroRef;
use proc_macro::bridge::WasmRuntimeRef;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::Mutex;
use wasmtime::TypedFunc;

extern crate proc_macro;

pub struct SharedRuntime {
    inner: Mutex<Runtime>,
}

#[derive(Default)]
struct Runtime {
    engine: wasmtime::Engine,
    macros: Vec<Macro>,
    wasm_files: Vec<WasmFile>,
}

struct WasmFile {
    instance: wasmtime::Instance,
    store: wasmtime::Store<wasi_common::WasiCtx>,
}

struct Macro {
    /// The index into `wasm_files` on `Runtime`.
    file_index: usize,
    macro_fn: TypedFunc<(), u32>,
}

impl WasmFile {
    fn new(runtime: &Runtime, path: &Path) -> Result<Self> {
        let mut linker = wasmtime::Linker::new(&runtime.engine);
        wasi_common::sync::add_to_linker(&mut linker, |s| s)?;

        let wasi = wasi_common::sync::WasiCtxBuilder::new()
            .inherit_stdio()
            .inherit_args()?
            .build();
        let mut store = wasmtime::Store::new(&runtime.engine, wasi);

        let module = wasmtime::Module::from_file(&runtime.engine, path)?;
        linker.module(&mut store, "", &module)?;
        linker
            .get_default(&mut store, "")?
            .typed::<(), ()>(&store)?
            .call(&mut store, ())?;

        let instance = linker.instantiate(&mut store, &module)?;

        Ok(Self { instance, store })
    }

    // This is just a proof-of-concept. It's not how we'd want things to actually work. Ideally we'd
    // call a single function that would return a pointer to some data that would contain
    // information about _all_ of the macros defined in the file. That data would contain the macro
    // names, types and addresses of the functions that do the work of the macro.
    fn get_macro_name(&mut self) -> Result<String> {
        let macro_name_len_fn = self
            .instance
            .get_typed_func::<(), u32>(&mut self.store, "macro_name_len")
            .context("Failed to get macro_name_len")?;
        let len = macro_name_len_fn.call(&mut self.store, ())? as usize;

        let macro_name_fn = self
            .instance
            .get_typed_func::<(), u32>(&mut self.store, "macro_name")
            .context("Failed to get macro_name")?;
        let ptr = macro_name_fn.call(&mut self.store, ())? as usize;

        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .context("Failed to get WASM memory")?;

        let data = memory.data(&self.store)[ptr..(ptr + len)].to_vec();
        Ok(String::from_utf8(data)?)
    }

    fn get_macro_fn(&mut self, name: &str) -> Result<TypedFunc<(), u32>> {
        self.instance.get_typed_func(&mut self.store, name)
    }
}

#[no_mangle]
pub extern "C" fn create_runtime() -> WasmRuntimeRef {
    let runtime = Box::new(SharedRuntime {
        inner: Mutex::new(Runtime::default()),
    });
    let runtime = Box::leak(runtime);
    unsafe { WasmRuntimeRef::new(runtime as *const SharedRuntime as usize) }
}

/// # Safety
/// Only intended to be called by rustc. `path_ptr` and `path_len` must represent a valid Path.
/// `runtime` must have been returned by `create_runtime`.
#[no_mangle]
pub unsafe extern "C" fn load_wasm_proc_macro(
    runtime_ref: WasmRuntimeRef,
    path_ptr: *const u8,
    path_len: usize,
) -> *const &'static [ProcMacro] {
    let runtime = unsafe { &*(runtime_ref.handle() as *const SharedRuntime) };
    let path_slice = unsafe { std::slice::from_raw_parts(path_ptr, path_len) };
    let path = Path::new(OsStr::from_bytes(path_slice));
    runtime
        .inner
        .lock()
        .map_err(|_| anyhow::anyhow!("Mutex poisoned"))
        .and_then(|mut lock| lock.load_wasm(path, runtime_ref))
        .map(|proc_macros| Box::leak(Box::new(proc_macros)) as *const &'static [ProcMacro])
        .unwrap_or(std::ptr::null())
}

// Currently this function accepts and returns TokenStreams. However the first thing we'd then want
// to do is serialise these to send to wasm. This is possibly wasteful, since they were already
// serialised and deserialized in order to get them this far. We could accept and return Buffers
// instead. That would require making Buffer public, which is why I didn't do it yet.
pub fn run_proc_macro(macro_ref: WasmProcMacroRef, buf: Buffer) -> Buffer {
    let runtime = unsafe { &*(macro_ref.runtime().handle() as *const SharedRuntime) };
    let mut runtime = runtime.inner.lock().unwrap();
    let runtime = &mut *runtime;
    let m = &runtime.macros[macro_ref.macro_id() as usize];
    let file = &mut runtime.wasm_files[m.file_index];
    let memory = file
        .instance
        .get_memory(&mut file.store, "memory")
        .expect("Failed to get WASM memory");


    let alloc = file.instance.get_typed_func::<u32, u32>(&mut file.store, "allocate").expect("Error getting alloc function from module");

    // copy data to wasm
    let ptr = alloc.call(&mut file.store, buf.len().try_into().unwrap()).expect("Error while calling alloc") as usize;
    let data = &mut memory.data_mut(&mut file.store)[ptr..(ptr + buf.len())];
    data.copy_from_slice(&buf[..]);

    // run macro
    let run_client = file.instance.get_typed_func::<u32, (u32, u32)>(&mut file.store, "run_macro").expect("Error getting run function from module");
    let res = run_client.call(&mut file.store, (ptr.try_into().unwrap(), buf.len())).expect("Error while calling run function");

    // get buffer
    let buf_len = res.0 as usize;
    let res_ptr = res.1 as usize;
    let mut data = memory.data(&file.store)[res_ptr..(res_ptr + buf_len)].to_vec();

    let mut index = -1;
    let mut pos = 0;

    for x in &data {
      if *x == b'!' {
        index = pos;
        break;
      }
      pos += 1;
    }
    if index >= 0 {
      data.truncate(index as usize);
    }

    Buffer::from(data)
}

impl Runtime {
    fn load_wasm(
        &mut self,
        path: &Path,
        runtime_ref: WasmRuntimeRef,
    ) -> Result<&'static [ProcMacro]> {
        let mut file = WasmFile::new(self, path)?;
        let file_index = self.wasm_files.len();

        let name = file.get_macro_name()?;

        // It's necessary to leak the macro name because currently rustc requires a `&'static str`
        // for all macro names.
        let name = name.leak();
        let macro_id = self.macros.len() as u32;
        let macro_ref = unsafe { WasmProcMacroRef::new(runtime_ref, macro_id) };
        let macros = vec![ProcMacro::wasm_bang(name, macro_ref, run_proc_macro)];

        let macro_fn = file.get_macro_fn(name)?;

        println!("Loaded wasm proc macro named `{name}`");

        self.wasm_files.push(file);
        self.macros.push(Macro {
            file_index,
            macro_fn,
        });

        Ok(macros.leak())
    }
}
