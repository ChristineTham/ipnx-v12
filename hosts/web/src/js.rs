//! **What the page's half of the machine supplies** — the kernel worker's
//! JavaScript (`hosts/web/www/kernel.js`), imported as module `host`.
//!
//! Plan 9's machine half is assembly and a few C functions per architecture
//! (`pc/l.s`, `pc/trap.c`); this one is a worker's JavaScript, because that is
//! what can reach a browser's workers, its memory and its clock. Each function
//! is named for what it does, and nothing here decides anything: the decisions
//! are the kernel's, and the Rust half of this machine (`machine.rs`).

#[link(wasm_import_module = "host")]
extern "C" {
    // ---- the console: the page's screen and keyboard ----
    /// `screenputs` — bytes for the screen.
    pub fn putstr(p: *const u8, n: usize);
    /// What has been typed since last asked, copied to `p`: how many bytes,
    /// or -1 when input has ended.
    pub fn kbd(p: *mut u8, cap: usize) -> i32;
    /// The interrupt key, pressed since last asked.
    pub fn interrupt() -> i32;
    /// Milliseconds since the epoch, with a fraction.
    pub fn now() -> f64;
    /// `n` bytes of the page's entropy.
    pub fn random(p: *mut u8, n: usize);
    /// `/dev/reboot`'s command; 0 when done.
    pub fn reboot(p: *const u8, n: usize) -> i32;
    /// The system is over: pid 1's status, or why it could not run.
    pub fn status(p: *const u8, n: usize, ok: i32);
    /// Wait up to `ms` — less if a key arrives.
    pub fn delay(ms: u32);

    // ---- the tree: the files the page serves ----
    /// Fetch a file of the built root: its length, or -1.
    pub fn fetch(p: *const u8, n: usize) -> i32;
    /// The bytes of the last fetch.
    pub fn fetched(p: *mut u8, n: usize);

    // ---- the machine: the processes' workers and memories ----
    /// An image made a module: its number, or -1 if it is not one.
    pub fn compile(p: *const u8, n: usize) -> i32;
    /// Give `pid` a new memory of `initial` pages, at most `maximum`, and a
    /// worker running `module` from `_start` with these arguments, each
    /// ended by a NUL — ending the worker it had, if it had one.
    pub fn start(pid: u32, module: i32, initial: u32, maximum: u32, args: *const u8, n: usize);
    /// Wait for `pid` to say something, up to `ms`: what it said — [`ev`] —
    /// or [`ev::TIME`] when the time ran out. Waiting 0 asks whether it is
    /// waiting on the kernel.
    pub fn wait(pid: u32, ms: i32) -> i32;
    /// Word `i` of what `pid` said: a call's number and its arguments, or an
    /// event's details.
    pub fn word(pid: u32, i: u32) -> i64;
    /// Answer `pid`: a [`rep`] kind, a value, and one more word.
    pub fn reply(pid: u32, kind: i32, value: i64, aux: i32);
    /// Bytes for `pid` to read — a note's text.
    pub fn xfer(pid: u32, p: *const u8, n: usize);
    /// Bytes `pid` left for the kernel — a fault's text: how many.
    pub fn xferin(pid: u32, p: *mut u8, cap: usize) -> i32;
    /// Copy from `pid`'s memory: 0, or -1 if any of it is outside.
    pub fn memread(pid: u32, addr: u32, p: *mut u8, n: usize) -> i32;
    /// Copy into `pid`'s memory: 0, or -1 if any of it is outside.
    pub fn memwrite(pid: u32, addr: u32, p: *const u8, n: usize) -> i32;
    /// The length of the string at `addr` in `pid`'s memory, or -1.
    pub fn strlen(pid: u32, addr: u32) -> i32;
    /// `*addr`; `ok` is set to 0 if it is outside.
    pub fn load(pid: u32, addr: u32, ok: *mut i32) -> i32;
    /// `cmpswap`: 1 swapped, 0 not, -1 outside.
    pub fn cas(pid: u32, addr: u32, old: i32, new: i32) -> i32;
    /// Make `child`'s memory from `parent`'s, which has unwound its stack
    /// for a fork: a copy from `lo`, the lowest live address; or for
    /// `RFMEM`, the parent's own.
    pub fn fork(parent: u32, child: u32, share: i32, lo: u32);
    /// Give `child` a worker that winds back the stack its parent unwound.
    pub fn child(pid: u32);
    /// `pid` is over: end its worker where it stands, and forget it.
    pub fn kill(pid: u32);
}

/// What a process says ([`wait`]).
pub mod ev {
    /// Nothing, in the time given.
    pub const TIME: i32 = 0;
    /// A call: word 0 its number (`sys.h`), words 1–5 its arguments.
    pub const CALL: i32 = 1;
    /// After a note handler: go on with the call it interrupted.
    pub const DELIVER: i32 = 2;
    /// Its image ran to the end, or its worker is gone.
    pub const EXITED: i32 = 3;
    /// It trapped; the text is in its transfer area.
    pub const FAULT: i32 = 4;
    /// It unwound for a fork: word 0 the child, 1 whether it shares, 2 the
    /// lowest live address, 3 its `Tos`, 4 the top of its stack region, 5
    /// that it could not unwind.
    pub const FORKED: i32 = 5;
    /// A fork's parent, wound back: `sysrfork`'s *"sched()"*.
    pub const YIELD: i32 = 6;
    /// Where it is, as [`super::rep::PC`] asked: word 7.
    pub const PC: i32 = 7;
}

/// How the kernel answers ([`reply`]).
pub mod rep {
    /// The call's value; go on.
    pub const RET: i32 = 0;
    /// Run the note handler at `aux` with the text in the transfer area,
    /// then say [`super::ev::DELIVER`]. The value is the call's.
    pub const HANDLER: i32 = 1;
    /// `noted(NCONT)`: leave the handler, back to where the note found it.
    pub const NOTED: i32 = 2;
    /// `rfork(RFPROC)` has made the child `value`, sharing the memory if
    /// `aux`: unwind the stack for it.
    pub const FORK: i32 = 3;
    /// The child's memory is made: wind the stack back.
    pub const GO: i32 = 4;
    /// Say where you are — the innermost frame of the image, its offset in
    /// the module — and go on waiting for the answer to the call.
    pub const PC: i32 = 5;
}
