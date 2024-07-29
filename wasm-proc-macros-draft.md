This experimental change / proof-of-concept changes rustc so that a bang-style proc-macro can be
loaded from a .wpm file (wasm file). It's super rough and there are plenty of hacks that need to be
resolved. Not the least of which is that it doesn't yet pass a TokenStream into the wasm code. The
main point however was to show one possible way in which rustc can load a shared object which
contains the wasm runtime, which then loads and runs the wasm code.

To try it out, first build your rust compiler. It needs to be available via rustup as stage1. e.g.
the following should work:

```sh
rustc +state1 --version
```

Next, built the proc-macro-loader:

```sh
cd wasm-proc-macro-loader
cargo build
```

The remaining build and run steps are condensed into a small script:

```sh
cd use-wasm-proc-macro
./build-and-run
```

If this works, you should see output like this:

```
   Finished `release` profile [optimized] target(s) in 0.00s
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.00s
Wasm proc-macro has been loaded
Loaded wasm proc macro named `make_answer`
Wasm proc-macro is being run
Answer: 42
```

This include a few notable things:

* Some print messages that came from the wasm code.
* Although the wasm code didn't get passed a TokenStream, it was the wasm code that decided that the
  answer should be 42.
