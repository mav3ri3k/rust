use std::path::Path;
use wasmtime::*;

fn runtime(ts: Vec<u8>, path: Path) -> Result<()> {
    // First the wasm module needs to be compiled. This is done with a global
    // "compilation environment" within an `Engine`. Note that engines can be
    // further configured through `Config` if desired instead of using the
    // default like this is here.
    let engine = Engine::default();
    let module = Module::from_file(&engine, path)?;

    // After a module is compiled we create a `Store` which will contain
    // instantiated modules and other items like host functions. A Store
    // contains an arbitrary piece of host information, and we use `MyState`
    // here.
    let mut store = Store::new(&engine);

    // Our wasm module we'll be instantiating requires one imported function.
    // the function takes no parameters and returns no results. We create a host
    // implementation of that function here, and the `caller` parameter here is
    // used to get access to our original `MyState` value.
    let hello_func = Func::wrap(&mut store, |mut caller: Caller<'_, MyState>| {
        caller.data_mut().count += 1;
    });

    // Once we've got that all set up we can then move to the instantiation
    // phase, pairing together a compiled module as well as a set of imports.
    // Note that this is where the wasm `start` function, if any, would run.
    let instance = Instance::new(&mut store, &module, &[])?;
    let exported_func: Func<Vec<u8>, Vec<u8>> = instance.get_func(&store, "exported_function")?;

    // Next we poke around a bit to extract the `run` function from the module.
    let run = instance.get_typed_func::<(), ()>(&mut store, "run")?;

    // And last but not least we can call it!
    run.call(&mut store, ())?;

    Ok(())
}
