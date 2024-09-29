# Sandboxed and Deterministic Proc Macro using Wasm

This branch: 'gsoc24' is the final snapshot for the state of code during the end of the GSoC project: [Sandboxed and Deterministic Proc Macro using Wasm](https://summerofcode.withgoogle.com/programs/2024/projects/kXG0mZoj)
The initial goal for the project was to:
Add experimental support to Rustc for building and running procedural macros as WebAssembly. Procedural Macro crates can opt-in to be compiled into WebAssembly. This wasm-proc-macro will be a WASM blob sandboxed using WASM. It will interact with the compiler only through a stream of tokens and will not be able to interact with the outside world.

The project was inspired by: [Build-time execution sandboxing](https://github.com/rust-lang/compiler-team/issues/475) and [Pre-RFC: Sandboxed, deterministic, reproducible, efficient Wasm compilation of proc macros](https://internals.rust-lang.org/t/pre-rfc-sandboxed-deterministic-reproducible-efficient-wasm-compilation-of-proc-macros/19359)

I started with the project in a top down fashion such that I started with the problem and slowly chipping away at it as required.
**Fire first aim later**.

Few notable decisions were taken early on based on discussions and advice from other community members so as to only focus on
`wasm32-unknown-unknown` is the wasm target, and the focus is on using Wasmtime as the runtime.

Over the course of the project, it was established that reaching the original goal would not be feasible within the
time constraints of a medium project and thus was much reduced.

## Notable Content
For exploring the project and work done, it is advised to check the [gsoc24 branch diff](https://github.com/rust-lang/rust/compare/master...mav3ri3k:rust:gsoc24).
Also please find a summary of relevant folders/files below.

```sh
 Rust
   │
   ├─▶ Compile
   │      │
   │      └▶ rustc_metadata: Used for loading wasm proc macro:
   │      │                  `.wpm` file and loading metadata
   │      │                 from a proc macro
   │      │
   │      └▶ rustc_expand: Used for expanding macros proc macro
   │
   ├─▶ Library
   │      │
   │      └▶ proc_macro/src/bridge: The bridge module contains all
   │                    │           the code related to moving
   │                    │           TokenStream between compiler
   │                    │           (server) and the proc macro (client)
   │                    │
   │                    │
   │                    └▶  bridge/client.rs: wasm proc macro is also
   │                                           a client and thus this
   │                                           file contains related
   │                                           changes
   │
   ├▶ use-wasm-proc-macro: Contains helper script for running
   │                        wasm proc macro
   │
   ├▶ wasm-proc-macro: Folder containing all client-side code for wasm
   │                   proc macro which compiler to wasm
   │
   └▶ wasm-proc-macro-loader: Contains the main logic for loading and
                               running wasm proc macro
```
## Project Walkthrough
Now I shall try to explain the project and decisions in detail:

### Loading wasm proc macro
The starting point in the lifecycle of any proc macro starts at `compiler/rustc_metadata`
where the crate for relevant proc macro is registered relevant, metadata is decoded, etc.

Now for wasm proc macros(wpm), we have chosen the extension `.wpm`. However in reality this is just the
`.wasm` file generated while compiling wasm proc macro to target `wasm32-unknown-unknown`. This also
means there is no actual metadata attached to the wasm proc macro file. Currently for correctly
registering wasm proc macro(wpm), a normal proc macro in `rust/pmacro1` is used to augment the metadata.

### Calling wasm proc macro
Once registered, the next part is the process of expanding wpm. This is handled in `compiler/rustc_expand`.
Originally there were 3 types of proc macros:
```rust
pub struct BangProcMacro
pub struct AttrProcMacro
pub struct DeriveProcMacro
```

We have introduced another type: `pub struct WasmBangProcMacro`. Instead of relying on/using original types,
we felt that the introduction of wasm proc macro was large enough that significant parts of the original
code pipe could not be used thus new type was used. Currently, every experiment is done with this one
new type. This was because extending the code for wpm from function type to attribute and derived type
is extremely trivial and can be handled in the future. Code for spawning the server, type conversion
from compiler's definition of tokenstream to proc macro's definition of the token stream also happens here.

Initially, there were tests to keep all the wasm runtime-related code also consolidated here in a slightly
similar fashion to mbe. However, it did not quite work and then my mentor scaffolded a setup to load
code for wasmtime runtime as a shared object. Thus this possibility was never further explored.

However, from my experience now, I feel api wise having wasm runtime-related code here is possible
and infact favorable since it would allow for easier use of various wasm-related dependencies and
forgo the hacky method of loading wasmtime crate as a shared object.

### Logic for wasm proc macro
The rest of proc macro-related logic lives in `library/proc_macro`. Even specifically we are only really
interested in code for the bridge module which handles communication between the compiler (server)
and the proc macro (client).

The core idea behind the current iteration of WPM is as follows:
The goal is to allow passing token streams between the compiler and wpm. It is generally not desirable
to directly pass rust types between wasm and rust due to ffi limitations. So the general practice is
to pass them around in serialized format. However, serialization is not as straightforward due
to complexities with span. The first experiment was to build a new custom encoding format similar to
one used by the [watt](https://github.com/dtolnay/watt/tree/master). However, there is already so
much code laid out for the almost same function so efforts were made to reuse the current
serialization mechanisms which overcome the span-related complexities/drawbacks through a RPC
mechanism.

However, as a shortcut, the buffer which is used for this serialization in `bridge::buffer` was made
public. Over several iterations, I have tried to reduce the amount of internal code I have made public.
However, these are just some shortcuts I took, and actually logic wise these can be cleaned in later iterations.

The core logic for expanding wpm lives in `proc_macro::bridge::client`. The core logic
related to interacting with the wasm file lives in `rust/wasm-proc-macro-loader`.

## Hacks
Since the project was approached in a goal-first, top-down fashion, a lot of hacks were used along the way.

### 1. Loading wasmtime
We can not have dependencies for lib proc_macro. This is due to how the code for
proc_macro is structured in the compiler and as of now, there is no way to circumvent it.
This was overcome with the help of my mentor who wrote the code for loading the wasmtime crate as a shared object.
The code itself is very hacky and uses leaking some stuff for it to correctly work.

### 2. Reading metadata
Currently `.wpm` is chosen as the extension for a wasm procedural macro file. However, it is a wasm
file and does not contain any metadata so a proxy proc macro is used for
loading metadata names `pmacro1`.

### 3. Passing TokenStream
It is best to only pass complex types between wasm objects in serialized formats.
In the current implementation of proc_macro TokenStreams have a set serialization format
and passed using a private `Buffer`. For easier / faster implementation this buffer was made public
and used directly for passing TokenStreams.

### 4. Hardcoded values
Currently, most values like function names for proc_macro are hardcoded.

## Current State
The code snapshot in the current state is still not fully complete. Upon running the code
following the step in `wasm-proc-macros-draft.md` the code compiles but errors out during
the final steps of running the wasm proc macro. This is because some code of RPC between
client-server depends on thread API in Rust which is not present for wasm32-unknown-unknown.
Parts of this code could not be reworked before the end of the GSoC period.

## Future Work
If you would like to start with code from the current state you are advised to go through
the `with_api` macro defined in lib proc_macro::bridge and all the times it is used/called.
This should give you an overall idea for the bridge api.

### Rework bridge
The current bridge depends on some thread API. As of right now, this is the final hurdle
before we can have a scaffolded working demo for WPM.
We need to either rework the bridge API to not depend on the thread or provide an alternate set
of APIs that are only used inside the wasm proc macro. From my assessment, for the logic
where thread local is used, it was hard to reason about not using it. The best option
seems to be providing an alternate set of API.

In my assessment, this can add up to enough for at least a small project.

### Build Target and  Metadata
Add proper support for a `wasm32-wpm` target with proper metadata support. This is dependent
on the API for wpm however relatively easier because compared to other crates, proc macro
do not have much to be added to the header and for the most part using the previous header/rlib
with a few minor tweaks during encoding would be sufficient.

### Compile Proc Macro to Wasm
Current expansion of proc macros during compilation can be seen using the utility:
[`cargo expand`](https://github.com/dtolnay/cargo-expand). This would need to be reworked for the target: `wasm32-wpm`.
It should expand to export appropriate functions for proc macro function names, bridge setup 
and wasm function table with corresponding proc macro functions. A reference implementation for only 
some of these features for the API are present in the current implementation.

### Better handle wasm runtime
From the work done, we should be able to load wasmtime as a dependency in rustc_expand
and retain most of the functionality as seen currently. This would avoid the hacky nature
of the current method. We can also look into receiving a path to wasm runtime by the compiler
rather than using only one runtime i.e. wasmtime at the moment.

## Problems I faced
I thought my current skill set was not being sufficiently challenged previously, so I took this
project as a real challenge. When I took the project I was informed that this project would likely
be a moonshot. I still took it because a moonshot is exciting to my little mind.
However, in hindsignt, this turned out to be a lot harder than I anticipated. Proc macros can not have any
dependency and so all the code here is hand-written low-level code, lower than I have ever been.
Also not having any dependencies means I can not just use my favorite crates for various jobs.

This was a major slip in my pre-project assessment which later caused me to reduce my goals
and increase the overall period which ended up overlapping during my college too.
This overlapping time has pushed me in terms of managing my time and prioritizing
things and focusing on maintaining a rigorous schedule.

## Learnings
Like I said it was a moonshot, a challenge I willingly took and I am very happy I did that. I actually
went through a somewhat legacy codebase, dug through it, and am currently in a place where I am pretty
confident with it when I make changes. When a panic or error happens, I generally know what caused it, and reading the core dump traces has become surprisingly readable.

I also became more familiar with various debugging/tracing tools like flamegraph, objdump
ptrace which I had to use a lot during the initial phase to understand
the working of proc macros.

In a personal capacity I also properly studied Category Theory for Programmers
which demystified the functional programming paradigm.

Understanding the functional paradigm was also one of the reasons I wanted to take
up a project in Rust along with others. Rust is also a low-level language with modern
tooling which allows for a healthier step to low-level code. I wanted to say it easier
but rust is not easy, however, it feels that way because while other languages
like C allow you to do anything, they also make it trivially easy to do and accumulate the wrong
habits and it is generally hard to find what the good habits are.

In that regard I feel that habits I have learned using rust and category theory have also
found their way into other code I have to write in other capacities like college, etc where
I have a better understanding of how I am structuring my code and thinking about the problem
and the logic.

Another wonderful thing I experienced was the incredible people. Rust is still not the
absolute mainstream language, but rapidly growing. Thus the people I met and saw in the Rust Zulip chat were some of the most amazing inspiring people I have had the privilege of being
with. They are extremely technical people and where inspiring to be around.

## Final Remarks
While I had overestimated the original goal for the project which had to be reduced later on,
I have found the experience to be very endearing. I wanted a challenge and it provided me
with a real one. I met various wonderful, inspirational people and made some new friends.
It has allowed me to break through from my previous technical prowess and reacher
better hights. Extremely happy and grateful that I was given a chance to be part of this
experience.
