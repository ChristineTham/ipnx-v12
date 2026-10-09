//! `ipnx-web` — Saranos in a browser.
//!
//! The same kernel as on a terminal, compiled to `wasm32-unknown-unknown`
//! and run in a worker of its own; each process a worker of its own
//! ([`machine`]); the files a page keeps ([`tree`]), served to the kernel by
//! the same 9P as a terminal's directory; and the page as the console. The
//! boot is `hosts/ipnx`'s, shared: [`ipnx::startboot_with`] does what
//! Plan 9's `boot` does, whichever host calls it.
//!
//! The page's side is `www/`: `kernel.js` runs this, `proc.js` runs a
//! process, and `saranos.js` is the page between them. On any target but
//! wasm32 this crate is only [`module`], so the workspace builds and tests
//! everywhere.

pub mod module;

#[cfg(target_arch = "wasm32")]
mod js;
#[cfg(target_arch = "wasm32")]
pub mod machine;
#[cfg(target_arch = "wasm32")]
pub mod tree;

#[cfg(target_arch = "wasm32")]
pub use page::{web_alloc, web_boot};

#[cfg(target_arch = "wasm32")]
mod page {
    use crate::js;
    use ipnx_kernel::devcons::Console;
    use std::rc::Rc;

    /// `conffile` (`pc/main.c:250`): what `$terminal` names, the file whose
    /// [`ipnx::LETTERS`] this host's device table is.
    pub const CONFFILE: &str = "hosts/web/src/lib.rs";

    /// **The page, as the console**: its screen, its keyboard, its clock.
    struct Page;

    impl Console for Page {
        /// `screenputs` (`devcons.c:12`): the bytes go to the page.
        fn putstrn(&mut self, s: &[u8]) {
            unsafe { js::putstr(s.as_ptr(), s.len()) }
        }

        /// What has been typed, without waiting. Input ends only when the
        /// page says so — a test's script, typed out — which `consread`
        /// takes as `^D`.
        fn kbdchars(&mut self) -> Option<Vec<u8>> {
            let mut b = vec![0u8; 4096];
            let n = unsafe { js::kbd(b.as_mut_ptr(), b.len()) };
            if n < 0 {
                return None;
            }
            b.truncate(n as usize);
            Some(b)
        }

        fn interrupt(&mut self) -> bool {
            unsafe { js::interrupt() != 0 }
        }

        fn now(&mut self) -> (u64, u64, u64) {
            let ns = (unsafe { js::now() } * 1e6) as u64;
            (ns, ns, 1_000_000_000)
        }

        /// The page's entropy (`crypto.getRandomValues`).
        fn random(&mut self, n: usize) -> Vec<u8> {
            let mut b = vec![0u8; n];
            unsafe { js::random(b.as_mut_ptr(), n) };
            b
        }

        fn drivers(&mut self) -> Vec<String> {
            vec!["web workers: the machine".into(), "a web page: the surface".into()]
        }

        fn memory(&mut self) -> (u64, u64, u64) {
            (0, 64 * 1024, 0)
        }

        /// `configfile[]` — the device table, in the configuration file's
        /// own shape, as `hosts/ipnx` gives it.
        fn config(&mut self) -> String {
            let mut s = format!("# {CONFFILE} - Saranos in a browser\ndev\n");
            for d in ipnx::LETTERS {
                s.push_str(&format!("\t{}\n", d.name()));
            }
            s
        }

        /// `/dev/reboot`: the page decides — `halt` ends the system's
        /// workers, and anything else is loading the page again.
        fn reboot(&mut self, cmd: &str) -> Result<(), String> {
            match unsafe { js::reboot(cmd.as_ptr(), cmd.len()) } {
                0 => Ok(()),
                _ => Err("this host reboots by being loaded again".into()),
            }
        }
    }

    /// **emca's file server — `#9/1`** — which is the page's: the window
    /// manager is the host's (docs/emca.md), and serves IPNX its files over
    /// 9P as the store is served (docs/surface.md). Every reply is the
    /// page's to give when it has it — a window's console read waits for a
    /// line — so each T-message is submitted and its reply harvested at the
    /// clock, as Plan 9's virtio9p harvests its queue (`#9`).
    struct Wsys;

    impl ipnx_kernel::devvirtio9p::Nineserver for Wsys {
        fn rpc(&mut self, _t: &[u8]) -> Result<Vec<u8>, String> {
            Err("the page answers when it has an answer".into())
        }

        fn submit(&mut self, t: &[u8]) -> Result<Option<Vec<u8>>, String> {
            unsafe { js::submit(t.as_ptr(), t.len()) };
            Ok(None)
        }

        fn harvest(&mut self) -> Vec<Vec<u8>> {
            let mut out = Vec::new();
            let mut b = vec![0u8; 64 * 1024];
            loop {
                let n = unsafe { js::harvest(b.as_mut_ptr(), b.len()) };
                if n == 0 {
                    return out;
                }
                if n < 0 {
                    b.resize((-n) as usize, 0);
                    continue;
                }
                out.push(b[..n as usize].to_vec());
            }
        }
    }

    /// Room in the kernel's memory for the page to write into: the
    /// command and the root's index, before [`web_boot`].
    #[no_mangle]
    pub extern "C" fn web_alloc(n: usize) -> *mut u8 {
        let mut b = vec![0u8; n];
        let p = b.as_mut_ptr();
        std::mem::forget(b);
        p
    }

    /// **Boot**, and return when the system is over — pid 1's status goes
    /// to the page. `cmd` is the command `init` runs, its words each ended
    /// by a NUL, or nothing for the interactive shell alone: the terminal
    /// host's command line (`ipnx::plan9ini`). `index` is the built root's
    /// index ([`crate::tree::Tree::new`]). `wm` says the page has a window
    /// manager, emca, which serves `#9/1`.
    ///
    /// # Safety
    /// Both are ranges [`web_alloc`] gave, written by the page.
    #[no_mangle]
    pub unsafe extern "C" fn web_boot(cmd: *const u8, ncmd: usize, index: *const u8, nindex: usize, wm: i32) {
        std::panic::set_hook(Box::new(|info| {
            let s = format!("ipnx-web: {info}\n");
            unsafe { js::putstr(s.as_ptr(), s.len()) };
        }));
        let cmd = String::from_utf8_lossy(std::slice::from_raw_parts(cmd, ncmd)).into_owned();
        let words: Vec<String> = cmd.split_terminator('\0').map(String::from).collect();
        let index = String::from_utf8_lossy(std::slice::from_raw_parts(index, nindex)).into_owned();
        let store = ipnx::store::Store::with(crate::tree::Tree::new(&index));
        let r = ipnx::startboot_with(
            Rc::new(crate::machine::Web::new()),
            CONFFILE,
            &ipnx::plan9ini(&words),
            Box::new(Page),
            // `#9/1` is emca's, where the page has a window manager: a page
            // with none is the console alone, as a terminal is
            if wm != 0 { vec![Box::new(store), Box::new(Wsys)] } else { vec![Box::new(store)] },
        );
        let (s, ok) = match r {
            Ok(s) => (s, 1),
            Err(e) => (e, 0),
        };
        js::status(s.as_ptr(), s.len(), ok);
    }
}
