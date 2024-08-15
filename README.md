
# SolGB EFRAME
This is the web frontend for my gameboy / gameboy color emulator.

## WASM Multithreading
My emulator is multithreaded, so getting it to run in a browser was a giant PITA. Using https://github.com/chemicstry/wasm_thread as a drop in replacement for std threads it required:
* Using the nightly compiler
* Build using the following config (enable some nightly only features and build std):
```
[target.'cfg(target_arch = "wasm32")']
rustflags = [
    # Enabled unstable APIs from web_sys
    "--cfg=web_sys_unstable_apis",
    # Enables features which are required for shared-memory
    "-C", "target-feature=+atomics,+bulk-memory,+mutable-globals",
    # Enables the possibility to import memory into wasm.
    # Without --shared-memory it is not possible to use shared WebAssembly.Memory.
    "-C", "link-args=--shared-memory --import-memory",
]

[unstable]
build-std = ["panic_abort", "std"]
```
* Setting COEP/COOP headers (Thanks https://github.com/gzuidhof/coi-serviceworker)
* No atomic waits on the main thread. I *think* this is the case now, but I had previosuly been sending some data to the GB and channel send (on unbounded channels) and try_send can both cause an atmoic waits under the right circumstances.
