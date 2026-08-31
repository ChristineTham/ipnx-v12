// shellwin.mjs — the `shell` window type: rc in a terminal pane, with the
// LINE EDITOR implemented by the host.
//
// This behaves like VS Code's terminal, and the difference from VS Code is the
// whole point: there, bash provides readline, so the shell owns the line
// editor. rc has no readline and is blissfully unaware — so the HOST provides
// it, which is "even command history and shell command line edit — host
// implemented" (design log, 2026-08-31).
//
// Output is output: the caret never enters it and it is not editable. You can
// still select and copy it — that is the terminal's own affordance, and it is
// what makes a session snarfable into another window.
//
// rc sees only complete lines. It never sees an arrow key, a backspace, or a
// history recall; none of that reaches the guest.

const ESC = "\x1b";

export function createShellWindow({ mount, send, makeTerm }) {
  const el = document.createElement("div");
  el.className = "term";
  el.style.cssText = "position:absolute;inset:0;";
  mount.appendChild(el);
  const t = makeTerm(el);
  const term = t.term;

  let line = "";        // the current input line — the host's, not rc's
  let pos = 0;          // caret position within it
  const hist = [];
  let hpos = 0;
  let stash = "";       // what was being typed before history was walked

  // repaint the input region only: back up over what is there, rewrite it,
  // erase the tail, then put the caret where it belongs.
  const setLine = (next, caret = next.length) => {
    const prevPos = pos;
    line = next;
    pos = Math.max(0, Math.min(caret, next.length));
    let s = "";
    if (prevPos > 0) s += `${ESC}[${prevPos}D`;
    s += line + `${ESC}[K`;
    const back = line.length - pos;
    if (back > 0) s += `${ESC}[${back}D`;
    term.write(s);
  };
  const left = (n) => { if (n > 0) { term.write(`${ESC}[${n}D`); pos -= n; } };
  const right = (n) => { if (n > 0) { term.write(`${ESC}[${n}C`); pos += n; } };

  term.onData((d) => {
    let i = 0;
    while (i < d.length) {
      const c = d[i];

      // ---- escape sequences: arrows, Home/End, Delete ----
      if (c === ESC && d[i + 1] === "[") {
        const m = d.slice(i + 2).match(/^(\d*)([A-D~HF])/);
        if (m) {
          i += 2 + m[0].length;
          const k = m[2], num = m[1];
          if (k === "A" || k === "B") {                 // history — the HOST's
            if (!hist.length) continue;
            if (hpos === hist.length) stash = line;
            hpos = Math.max(0, Math.min(hist.length, hpos + (k === "A" ? -1 : 1)));
            setLine(hpos === hist.length ? stash : hist[hpos]);
          } else if (k === "C") { if (pos < line.length) right(1); }
          else if (k === "D") { if (pos > 0) left(1); }
          else if (k === "H" || num === "1") { left(pos); }
          else if (k === "F" || num === "4") { right(line.length - pos); }
          else if (num === "3" && k === "~") {           // Delete
            if (pos < line.length) setLine(line.slice(0, pos) + line.slice(pos + 1), pos);
          }
          continue;
        }
      }

      i++;
      if (c === "\r" || c === "\n") {
        term.write("\r\n");
        if (line.trim()) hist.push(line);
        hpos = hist.length; stash = "";
        send(line + "\n");
        line = ""; pos = 0;
        continue;
      }
      if (c === "\x7f" || c === "\b") {                  // Backspace
        if (pos > 0) setLine(line.slice(0, pos - 1) + line.slice(pos), pos - 1);
        continue;
      }
      if (c === "\x01") { left(pos); continue; }                                  // ^A
      if (c === "\x05") { right(line.length - pos); continue; }                   // ^E
      if (c === "\x15") { setLine("", 0); continue; }                             // ^U
      if (c === "\x0b") { setLine(line.slice(0, pos), pos); continue; }           // ^K
      if (c === "\x03") { term.write("^C\r\n"); line = ""; pos = 0; hpos = hist.length; continue; }
      if (c === "\x04") { if (!line) send("\x04"); continue; }                    // ^D
      if (c >= " " || c === "\t") {                      // ordinary text
        setLine(line.slice(0, pos) + c + line.slice(pos), pos + 1);
      }
    }
  });

  return {
    term,
    // rc's output. It lands as output — not as text the caret can enter.
    write(text) { term.write(text.replace(/\n/g, "\r\n")); },
    focus: () => term.focus(),
    fit: () => t.fit.fit(),
    destroy: () => { try { term.dispose(); } catch { /* already gone */ } el.remove(); },
  };
}
