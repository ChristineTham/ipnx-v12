# WebKit repro: concurrent module-worker loads through a service worker fail

**Filed: [WebKit bug 322883](https://bugs.webkit.org/show_bug.cgi?id=322883)**
(2026-08-29, component Service Workers).
Found 2026-08-29 while shipping <https://christham.net/ipnx-v12/> (a wasm
system that spawns one module Worker per process) on GitHub Pages behind a
COOP/COEP-injecting service worker.

## Summary

On a page controlled by a service worker whose fetch handler calls
`respondWith(fetch(request))` (with or without header rewriting, with or
without CacheStorage), creating a dedicated **module** worker while another
module worker's script load is still in flight makes the later load fail:
the `Worker` object fires a detail-free `error` event (no message, no
filename, no lineno — a plain `Event`, not an `ErrorEvent`), and nothing
inside the worker global ever runs. Sequential creations are 100% reliable.
The failure needs only concurrency ≥ 2 and is deterministic at 2.

## Environment

- Safari 26.6.2, macOS 26.6.2 (build 25G83)
- Reproduces on `http://localhost` (plain static file server, HTTP/1.1)
- Not reproducible in Chromium (Chrome 148: 300/300 in every mode)

## Steps

1. Serve this directory statically (`python3 -m http.server` works).
2. Open `matrix.html?conc=1&n=300` — first visit registers `sw.js` and
   reloads under its control; the page then creates 300 module workers with
   the given concurrency, each terminated after its first message.
3. Repeat with `?conc=2&n=300` and `?conc=8&n=300`.
4. Control: `?nosw=1&conc=8&n=300` unregisters the SW and repeats.

## Results (measured 2026-08-29)

| mode | result |
|---|---|
| `conc=1`, via SW | **300/300 ok** |
| `conc=2`, via SW | **150/300 ok — every odd-indexed load fails** (the one that starts while another is in flight) |
| `conc=8`, via SW | **92/300 ok** |
| `conc=8`, no SW | **300/300 ok** |

`index.html` is the original single-mode version (sequential; passes).

## Expected

Concurrent module-worker script loads through a controlled page succeed, as
they do without the service worker and as they do in other engines.

## Actual

Every module-worker script load that begins while another is in flight
through the service worker fails with a detail-free `error` event at the
`Worker` object; the worker global never starts (an error/unhandledrejection
listener injected at the top of the worker script never fires).

## Workaround

Serialize module-worker creation (one script load in flight at a time,
released on the worker's first message/error). Applied in the demo above.
