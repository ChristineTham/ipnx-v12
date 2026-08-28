# hosts/browser — the Rust core compiled to wasm (implementation M5)

The kernel crate is a single-threaded pure state machine with no OS
dependencies — built to compile to wasm32 and run in a page. This host embeds
it in a JS shim structurally parallel to hosts/macos: Workers as processes,
the SAB mailbox, COOP/COEP from a static server. When it reaches the
conformance floor, the frozen JS kernel in poc/ serves as oracle only.
