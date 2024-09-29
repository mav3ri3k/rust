This experimental change / proof-of-concept changes rustc so that a bang-style proc-macro can be
loaded from a .wpm file (wasm file). It's super rough and there are plenty of hacks that need to be
resolved. **In fact the code will error out while running the wasm proc macro. This is because some
code for proc macro bridge rpc depends on thread api which is not available for target `wasm32-unknown-unknown`
and reworking the thread api dependent code turned out to be more time consuming that available
during the gsoc project time period. Thus it remains as the final hurdle before the rest of the code
infrastruce here can properly run.**

To try it out, first build your rust compiler. It needs to be available via rustup as stage1. e.g.
the following should work:

```sh
rustc +state1 --version
```

The remaining build and run steps are condensed into a small script:

```sh
cd use-wasm-proc-macro
./build-and-run
```

*In current state, the above code will panic while calling the thread api for proc macro rpc.*
