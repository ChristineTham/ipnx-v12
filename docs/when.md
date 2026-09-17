# When — what is built, and what is not
**Role: a *when* — the single authoritative statement of build status.** No
other document carries it.

## 2026-09-17 — rebuilt from scratch

The previous implementation was set aside entire: it had been shaped by the
superseded artifact, and continuing it meant carrying that design forward.
What exists now was built from the design and from `plan9/`, and nothing else.

| | |
|---|---|
| `kernel/` | `Chan`, the device table (Plan 9's `struct Dev`), the namespace keyed by channel identity, the process table with `rfork`'s share/copy/clear, the 9P2000 codec |
| `hosts/ipnx/` | a stub: it reports that it has no engine yet |
| `conformance/` | the distance to the demo |

**Functional equivalence to the demo: 0 of 12.** Nothing on the checklist is
reached. The kernel's rules are encoded and tested — 15 tests — but no process
has ever run: there is no `exec`, no device implemented, no userspace.

The phases are in [implementation.md](implementation.md). P0 is done; P1 is
`exec`.
