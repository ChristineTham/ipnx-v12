// shellwin.mjs — the `shell` window type. rc in an editable buffer.
//
// NOT a terminal, and not a "transcript" — Pike's word (typescript) is for the
// thing being replaced, so naming the new thing after the old thing's shape was
// backwards. This is a shell window: ONE EDITABLE BUFFER, a mark separating
// what has happened from what you are typing, Enter sending the line, and
// everything above ordinary editable, searchable, selectable text. acme's
// win(1) was this design's prototype twenty years early.
//
// rc knows none of this. It reads lines and writes bytes. Command-line
// editing, history recall, selection, copy and mouse editing are ALL the
// host's — which is "editing is the surface's" applied to the shell, and it
// is why there is no line discipline, no escape sequences and no terminal
// emulation anywhere in this file.
//
// The mark discipline, which is the only subtle part: output arriving while
// the user is mid-line is inserted AT THE MARK, so the half-typed command
// stays at the end where the cursor is. con(1) calls this its shadow-state
// mark arithmetic, and it is load-bearing.

import { EditorView, minimalSetup } from "../vendor/codemirror.bundle.mjs";

export function createShellWindow({ mount, send }) {
  let mark = 0;                 // where the input region begins
  const hist = [];              // command history — the host's, not rc's
  let hpos = 0;

  const view = new EditorView({
    doc: "",
    parent: mount,
    extensions: [
      minimalSetup,
      EditorView.lineWrapping,
      EditorView.theme({
        "&": { height: "100%", fontSize: "13px", background: "#0f1720", color: "#d6dde4" },
        ".cm-content": { fontFamily: "ui-monospace, 'SF Mono', Menlo, monospace", caretColor: "#9eeeee" },
        ".cm-cursor": { borderLeftColor: "#9eeeee" },
        "&.cm-focused": { outline: "none" },
        ".cm-selectionBackground, ::selection": { background: "#2d4155" },
      }),
    ],
  });

  // CAPTURE PHASE, deliberately: CodeMirror's own keymap also listens on
  // keydown, and this bundle exports neither `keymap` nor `Prec`, so the only
  // way to be certain Enter means "send the line" rather than "insert a
  // newline" is to run before the editor does.
  view.dom.addEventListener("keydown", (e) => {
    const doc = view.state.doc;
    const end = doc.length;
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      e.stopPropagation();
      const line = doc.sliceString(mark, end);
      view.dispatch({ changes: { from: end, insert: "\n" }, selection: { anchor: end + 1 } });
      mark = view.state.doc.length;
      if (line.trim()) { hist.push(line); hpos = hist.length; }
      send(line + "\n");
      return;
    }
    // history is the HOST's: rc never sees an arrow key
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      if (!hist.length) return;
      if (view.state.selection.main.head < mark) return;   // editing scrollback
      e.preventDefault();
      e.stopPropagation();
      hpos += e.key === "ArrowUp" ? -1 : 1;
      hpos = Math.max(0, Math.min(hist.length, hpos));
      const want = hpos === hist.length ? "" : hist[hpos];
      view.dispatch({ changes: { from: mark, to: view.state.doc.length, insert: want },
                      selection: { anchor: mark + want.length } });
      return;
    }
    // Backspace must not eat what has already happened
    if (e.key === "Backspace" && view.state.selection.main.empty &&
        view.state.selection.main.head <= mark) { e.preventDefault(); e.stopPropagation(); }
  }, true);


  return {
    view,
    // rc's output. Inserted AT THE MARK so a half-typed line stays put.
    write(text) {
      const sel = view.state.selection.main;
      const tail = view.state.doc.sliceString(mark, view.state.doc.length);
      const grew = text.length;
      view.dispatch({
        changes: { from: mark, insert: text },
        selection: { anchor: Math.min(sel.anchor + grew, view.state.doc.length + grew) },
        scrollIntoView: true,
      });
      mark += grew;
      // keep the caret in the input region if it was there
      if (sel.anchor >= mark - grew) {
        const want = Math.min(sel.anchor + grew, view.state.doc.length);
        view.dispatch({ selection: { anchor: want }, scrollIntoView: true });
      }
      void tail;
    },
    focus: () => view.focus(),
    destroy: () => view.destroy(),
  };
}
