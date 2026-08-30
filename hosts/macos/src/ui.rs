// M3: the presentation layer — one host window per '#w' window, blitting the
// kernel's own backing stores (the same bytes the raster tests read) and
// injecting mouse/keyboard through the same paths the wctl tests use. The
// kernel never learns there is a screen: frames arrive as Effect::WinUpdate,
// text as Effect::WinText, input leaves as Ev::{WinKey,WinMouse,WinClose}.
//
// Window-cons text is rendered by a deliberately small glass tty (append,
// wrap, newline, backspace, local echo) using the rootfs's own 9x18 fixed
// subfont — the font the system already owns, parsed from the k1 image
// format we ship. Buttons: left=1, right=3, middle=2, with option-left=2 and
// command-left=3 (the demo's chords, so one laptop trackpad drives acme).

use crate::Ev;
use kernel::CvSnap;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

pub enum UiMsg {
    Update { wid: u32, label: String, x: i32, y: i32, w: i32, h: i32, rgba: Vec<u8> },
    Text { wid: u32, bytes: Vec<u8> },
    Canvas { wid: u32, label: String, x: i32, y: i32, w: i32, h: i32, snap: Vec<CvSnap> },
    Gone { wid: u32 },
    Shutdown,
}

// ---- the rootfs's fixed subfont, parsed from k1 ----
pub struct Subfont {
    pub glyph_w: usize, // advance of a full cell (from the space glyph)
    pub glyph_h: usize,
    bwidth: usize,   // bytes per bitmap row (whole strip)
    rows: Vec<u8>,   // glyph_h rows of bwidth bytes
    fx: Vec<u32>,    // fontchar x per glyph (n+1 entries) — NOT fixed-cell
    fw: Vec<u8>,     // fontchar width per glyph
    n: usize,
}

impl Subfont {
    pub fn load(path: &std::path::Path) -> Option<Subfont> {
        let b = std::fs::read(path).ok()?;
        if b.len() < 60 {
            return None;
        }
        let field = |i: usize| -> i64 {
            let s = String::from_utf8_lossy(&b[i * 12..i * 12 + 12]);
            s.trim().parse().unwrap_or(0)
        };
        // header: chan[12] r.min.x r.min.y r.max.x r.max.y (each 12 bytes)
        let chan = String::from_utf8_lossy(&b[0..12]).trim().to_string();
        if chan != "k1" {
            return None;
        }
        let (x0, y0, x1, y1) = (field(1), field(2), field(3), field(4));
        let w = (x1 - x0) as usize;
        let h = (y1 - y0) as usize;
        let bwidth = (w + 7) / 8;
        let data_off = 60;
        let data_len = bwidth * h;
        if b.len() < data_off + data_len + 36 {
            return None;
        }
        let rows = b[data_off..data_off + data_len].to_vec();
        // subfont trailer: n[12] height[12] ascent[12] then fontchar entries;
        // our mksubfont/cload output is fixed-cell, so x = i * glyph_w
        let t = data_off + data_len;
        let tf = |i: usize| -> i64 {
            let s = String::from_utf8_lossy(&b[t + i * 12..t + i * 12 + 12]);
            s.trim().parse().unwrap_or(0)
        };
        let n = tf(0) as usize;
        let height = tf(1) as usize;
        if n == 0 || height == 0 {
            return None;
        }
        // fontchar array: n+1 entries of 6 bytes — x[u16le] top bottom left width
        let fc = t + 36;
        if b.len() < fc + (n + 1) * 6 {
            return None;
        }
        let mut fx = Vec::with_capacity(n + 1);
        let mut fw = Vec::with_capacity(n + 1);
        for i in 0..=n {
            let e = fc + i * 6;
            fx.push(b[e] as u32 | (b[e + 1] as u32) << 8);
            fw.push(b[e + 5]);
        }
        let cell = *fw.iter().max().unwrap_or(&9) as usize;
        Some(Subfont { glyph_w: cell.max(1), glyph_h: h, bwidth, rows, fx, fw, n })
    }

    fn bit(&self, glyph: usize, gx: usize, gy: usize) -> bool {
        if glyph >= self.n || gy >= self.glyph_h {
            return false;
        }
        if gx >= self.fw[glyph] as usize {
            return false;
        }
        let x = self.fx[glyph] as usize + gx;
        if x >= (self.fx[glyph + 1] as usize).max(self.fx[glyph] as usize) && self.fw[glyph] == 0 {
            return false;
        }
        let byte = self.rows[gy * self.bwidth + x / 8];
        byte & (0x80 >> (x % 8)) != 0
    }
}

// ---- a very small tty: lines of text over the window's frame ----
struct Tty {
    lines: Vec<String>,
}

impl Tty {
    fn feed(&mut self, bytes: &[u8]) {
        if self.lines.len() > 2000 {
            let cut = self.lines.len() - 2000;
            self.lines.drain(0..cut);        // the glass tty is not a log
        }
        for &c in bytes {
            match c {
                b'\n' => self.lines.push(String::new()),
                8 | 127 => {
                    if let Some(l) = self.lines.last_mut() {
                        l.pop();
                    }
                }
                b'\r' => {}
                _ => {
                    if self.lines.is_empty() {
                        self.lines.push(String::new());
                    }
                    self.lines.last_mut().unwrap().push(c as char);
                }
            }
        }
    }
}

struct WinState {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    w: i32,
    h: i32,
    frame: Vec<u8>, // r8g8b8a8, w*h*4, the kernel's backing
    tty: Tty,
    has_text: bool,
    buttons: i32,
    mx: i32,
    my: i32,
    cv: Option<Vec<CvSnap>>,
    cv_hits: Vec<CvHit>,
    cv_edit: Option<u32>,
}

// a clickable region of the rendered canvas: verb 1 execute, 2 look
struct CvHit {
    x0: i32, y0: i32, x1: i32, y1: i32,
    id: u32, verb: u8, blen: usize, text: String,
}

fn cv_q(t: &str) -> String {
    t.replace('%', "%25").replace(' ', "%20").replace('\n', "%0A")
}

fn cv_attr<'a>(n: &'a CvSnap, key: &str) -> Option<&'a str> {
    n.attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn cv_children<'a>(snap: &'a [CvSnap], parent: u32) -> Vec<&'a CvSnap> {
    let pid = parent.to_string();
    let mut kids: Vec<&CvSnap> = snap.iter()
        .filter(|n| n.id != 0 && cv_attr(n, "parent").unwrap_or("0") == pid)
        .collect();
    kids.sort_by_key(|n| (cv_attr(n, "order").and_then(|o| o.parse::<i64>().ok()).unwrap_or(0), n.id));
    kids
}

fn cv_str(buf: &mut [u8], w: usize, h: usize, font: &Subfont, x: i32, y: i32, s: &str) -> (i32, i32) {
    let (gw, gh) = (font.glyph_w, font.glyph_h);
    let mut maxw = 0i32;
    let mut rows = 0i32;
    for line in s.split('\n') {
        let mut col = 0i32;
        for ch in line.chars() {
            let g = ch as usize;
            if (0x20..0x7f).contains(&g) {
                for gy in 0..gh {
                    for gx in 0..gw {
                        if font.bit(g, gx, gy) {
                            let px = x + col * gw as i32 + gx as i32;
                            let py = y + rows * gh as i32 + gy as i32;
                            if px >= 0 && py >= 0 && (px as usize) < w && (py as usize) < h {
                                let o = (py as usize * w + px as usize) * 4;
                                buf[o] = 0; buf[o + 1] = 0; buf[o + 2] = 0;
                            }
                        }
                    }
                }
            }
            col += 1;
        }
        maxw = maxw.max(col * gw as i32);
        rows += 1;
    }
    (maxw, rows.max(1) * gh as i32)
}

fn cv_box(buf: &mut [u8], w: usize, h: usize, x0: i32, y0: i32, x1: i32, y1: i32) {
    let mut px = |x: i32, y: i32| {
        if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
            let o = (y as usize * w + x as usize) * 4;
            buf[o] = 160; buf[o + 1] = 160; buf[o + 2] = 176;
        }
    };
    for x in x0..=x1 { px(x, y0); px(x, y1); }
    for y in y0..=y1 { px(x0, y); px(x1, y); }
}

// lay the tree out: stacks flow col/row, text draws, action nodes get a
// border and a hit region, the last edit is the keyboard's target.
fn cv_render(buf: &mut [u8], w: usize, h: usize, font: &Subfont, snap: &[CvSnap],
             hits: &mut Vec<CvHit>, edit: &mut Option<u32>, id: u32, x: i32, y: i32) -> (i32, i32) {
    let node = match snap.iter().find(|n| n.id == id) { Some(n) => n, None => return (0, 0) };
    match node.kind {
        0 => {
            let row = cv_attr(node, "dir") == Some("row");
            let (mut cx, mut cy) = (x, y);
            let (mut tw, mut th) = (0i32, 0i32);
            for k in cv_children(snap, id) {
                let (cw, ch) = cv_render(buf, w, h, font, snap, hits, edit, k.id, cx, cy);
                if row {
                    cx += cw + 8;
                    tw += cw + 8;
                    th = th.max(ch);
                } else {
                    cy += ch + 6;
                    th += ch + 6;
                    tw = tw.max(cw);
                }
            }
            (tw, th)
        }
        3 => (0, 0), // path: not rendered on this surface yet (recorded)
        _ => {
            let text = String::from_utf8_lossy(&node.data).into_owned();
            let action = cv_attr(node, "action");
            let pad = if action.is_some() { 5 } else { 0 };
            let (sw, sh) = cv_str(buf, w, h, font, x + pad, y + pad, &text);
            let (bw, bh) = (sw + pad * 2, sh + pad * 2);
            if let Some(a) = action {
                cv_box(buf, w, h, x, y, x + bw.max(10), y + bh);
                hits.push(CvHit {
                    x0: x, y0: y, x1: x + bw.max(10), y1: y + bh,
                    id: node.id, verb: if a == "look" { 2 } else { 1 },
                    blen: node.data.len(), text,
                });
            }
            if node.kind == 2 {
                *edit = Some(node.id);
            }
            (bw, bh)
        }
    }
}

pub struct App {
    ev: Sender<Ev>,
    ctx: Option<softbuffer::Context<Rc<Window>>>,
    wins: HashMap<u32, WinState>,
    by_window: HashMap<WindowId, u32>,
    font: Option<Subfont>,
    modifiers: winit::keyboard::ModifiersState,
}

impl App {
    pub fn new(ev: Sender<Ev>, font: Option<Subfont>) -> App {
        App { ev, ctx: None, wins: HashMap::new(), by_window: HashMap::new(), font,
              modifiers: Default::default() }
    }

    fn paint(&mut self, wid: u32) {
        let font = &self.font;
        let Some(ws) = self.wins.get_mut(&wid) else { return };
        let scale = ws.window.scale_factor();
        let (pw, ph) = (
            (ws.w as f64 * scale) as u32,
            (ws.h as f64 * scale) as u32,
        );
        let (Some(npw), Some(nph)) = (NonZeroU32::new(pw.max(1)), NonZeroU32::new(ph.max(1))) else { return };
        if ws.surface.resize(npw, nph).is_err() {
            return;
        }
        let Ok(mut buf) = ws.surface.buffer_mut() else { return };
        let (w, h) = (ws.w as usize, ws.h as usize);
        // compose: frame, then the tty text if any has arrived
        let mut composed: Vec<u8>;
        let src: &[u8] = if let Some(snap) = ws.cv.clone() {
            composed = vec![255u8; w * h * 4];
            let mut hits = Vec::new();
            let mut edit = None;
            if let Some(f) = font {
                cv_render(&mut composed, w, h, f, &snap, &mut hits, &mut edit, 0, 8, 8);
            }
            ws.cv_hits = hits;
            ws.cv_edit = edit;
            &composed
        } else if ws.has_text {
            composed = ws.frame.clone();
            if let Some(f) = font {
                let (gw, gh) = (f.glyph_w, f.glyph_h);
                let cols = (w / gw).max(1);
                let rows = (h / gh).max(1);
                let start = ws.tty.lines.len().saturating_sub(rows);
                for (row, line) in ws.tty.lines[start..].iter().enumerate() {
                    for (col, ch) in line.chars().take(cols).enumerate() {
                        let g = ch as usize;
                        if !(0x20..0x7f).contains(&g) {
                            continue;
                        }
                        for gy in 0..gh {
                            for gx in 0..gw {
                                if f.bit(g, gx, gy) {   // glyph index == codepoint
                                    let px = col * gw + gx;
                                    let py = row * gh + gy;
                                    if px < w && py < h {
                                        let o = (py * w + px) * 4;
                                        composed[o] = 0;
                                        composed[o + 1] = 0;
                                        composed[o + 2] = 0;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            &composed
        } else {
            &ws.frame
        };
        // nearest-neighbour blit logical -> physical, r8g8b8a8 -> 0RGB
        for py in 0..ph as usize {
            let sy = ((py as f64) / scale) as usize;
            let sy = sy.min(h.saturating_sub(1));
            for px in 0..pw as usize {
                let sx = ((px as f64) / scale) as usize;
                let sx = sx.min(w.saturating_sub(1));
                let o = (sy * w + sx) * 4;
                let (r, g, b) = if o + 2 < src.len() {
                    (src[o] as u32, src[o + 1] as u32, src[o + 2] as u32)
                } else {
                    (255, 255, 255)
                };
                buf[py * pw as usize + px] = (r << 16) | (g << 8) | b;
            }
        }
        let _ = buf.present();
    }
}


impl ApplicationHandler<UiMsg> for App {
    fn resumed(&mut self, _el: &ActiveEventLoop) {}

    fn user_event(&mut self, el: &ActiveEventLoop, msg: UiMsg) {
        match msg {
            UiMsg::Update { wid, label, x, y, w, h, rgba } => {
                if !self.wins.contains_key(&wid) {
                    // the kernel's cascade places new windows; a menu-bar
                    // offset keeps window 1 out from under it
                    let attrs = Window::default_attributes()
                        .with_title(label.clone())
                        .with_position(winit::dpi::LogicalPosition::new(
                            (x + 60) as f64, (y + 60) as f64))
                        .with_inner_size(LogicalSize::new(w as f64, h as f64));
                    let Ok(window) = el.create_window(attrs) else { return };
                    let window = Rc::new(window);
                    if self.ctx.is_none() {
                        self.ctx = softbuffer::Context::new(window.clone()).ok();
                    }
                    let Some(ctx) = &self.ctx else { return };
                    let Ok(surface) = softbuffer::Surface::new(ctx, window.clone()) else { return };
                    self.by_window.insert(window.id(), wid);
                    self.wins.insert(wid, WinState {
                        window, surface, w, h, frame: rgba,
                        tty: Tty { lines: vec![String::new()] },
                        has_text: false, buttons: 0, mx: 0, my: 0,
                        cv: None, cv_hits: Vec::new(), cv_edit: None,
                    });
                } else if let Some(ws) = self.wins.get_mut(&wid) {
                    ws.window.set_title(&label);
                    if (ws.w, ws.h) != (w, h) {
                        ws.w = w;
                        ws.h = h;
                        let _ = ws.window.request_inner_size(LogicalSize::new(w as f64, h as f64));
                    }
                    ws.frame = rgba;
                }
                self.paint(wid);
                let _ = self.ev.send(Ev::WinAck { wid });
            }
            UiMsg::Canvas { wid, label, x, y, w, h, snap } => {
                if !self.wins.contains_key(&wid) {
                    // a canvas-first window: create it exactly as Update would
                    let attrs = Window::default_attributes()
                        .with_title(label.clone())
                        .with_position(winit::dpi::LogicalPosition::new(
                            (x + 60) as f64, (y + 60) as f64))
                        .with_inner_size(LogicalSize::new(w as f64, h as f64));
                    if let Ok(window) = el.create_window(attrs) {
                        let window = Rc::new(window);
                        if self.ctx.is_none() {
                            self.ctx = softbuffer::Context::new(window.clone()).ok();
                        }
                        if let Some(ctx) = &self.ctx {
                            if let Ok(surface) = softbuffer::Surface::new(ctx, window.clone()) {
                                self.by_window.insert(window.id(), wid);
                                self.wins.insert(wid, WinState {
                                    window, surface, w, h, frame: Vec::new(),
                                    tty: Tty { lines: vec![String::new()] },
                                    has_text: false, buttons: 0, mx: 0, my: 0,
                                    cv: None, cv_hits: Vec::new(), cv_edit: None,
                                });
                            }
                        }
                    }
                }
                if let Some(ws) = self.wins.get_mut(&wid) {
                    ws.window.set_title(&label);
                    ws.cv = Some(snap);
                    self.paint(wid);
                    let _ = self.ev.send(Ev::WinAck { wid }); // credit returns after paint
                }
            }
            UiMsg::Text { wid, bytes } => {
                if let Some(ws) = self.wins.get_mut(&wid) {
                    ws.has_text = true;
                    ws.tty.feed(&bytes);
                    self.paint(wid);
                }
            }
            UiMsg::Gone { wid } => {
                if let Some(ws) = self.wins.remove(&wid) {
                    self.by_window.remove(&ws.window.id());
                }
            }
            UiMsg::Shutdown => el.exit(),
        }
    }

    fn window_event(&mut self, _el: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        let Some(&wid) = self.by_window.get(&window_id) else { return };
        match event {
            WindowEvent::RedrawRequested => self.paint(wid),
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::CloseRequested => {
                if self.wins.get(&wid).map(|w| w.cv.is_some()).unwrap_or(false) {
                    // canvas close is advisory: the app decides (wctl delete removes)
                    let _ = self.ev.send(Ev::WinCanvasEv { wid, line: "close 0".into() });
                    return;
                }
                let _ = self.ev.send(Ev::WinClose { wid });
                if let Some(ws) = self.wins.remove(&wid) {
                    self.by_window.remove(&ws.window.id());
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(ws) = self.wins.get_mut(&wid) {
                    let scale = ws.window.scale_factor();
                    ws.mx = (position.x / scale) as i32;
                    ws.my = (position.y / scale) as i32;
                    if ws.cv.is_none() {
                        let _ = self.ev.send(Ev::WinMouse { wid, x: ws.mx, y: ws.my, b: ws.buttons });
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = state == ElementState::Pressed;
                let bit = match button {
                    MouseButton::Left if self.modifiers.alt_key() => 2,
                    MouseButton::Left if self.modifiers.super_key() => 4,
                    MouseButton::Left => 1,
                    MouseButton::Middle => 2,
                    MouseButton::Right => 4,
                    _ => 0,
                };
                if let Some(ws) = self.wins.get_mut(&wid) {
                    if ws.cv.is_some() {
                        // the input convention: left activates; the hit's role
                        // decides the verb (execute or look)
                        if pressed && bit == 1 {
                            let (mx, my) = (ws.mx, ws.my);
                            if let Some(hit) = ws.cv_hits.iter()
                                .find(|t| mx >= t.x0 && mx <= t.x1 && my >= t.y0 && my <= t.y1) {
                                let verb = if hit.verb == 2 { "look" } else { "execute" };
                                let line = format!("{} {} 0 {} {}", verb, hit.id, hit.blen, cv_q(&hit.text));
                                let _ = self.ev.send(Ev::WinCanvasEv { wid, line });
                            }
                        }
                        return;
                    }
                    if pressed {
                        ws.buttons |= bit;
                    } else {
                        // releases clear the chorded bit AND the plain bit:
                        // the user may lift option before the button
                        ws.buttons &= !(bit | 1);
                    }
                    let _ = self.ev.send(Ev::WinMouse { wid, x: ws.mx, y: ws.my, b: ws.buttons });
                }
            }
            WindowEvent::KeyboardInput { event: KeyEvent { logical_key, state: ElementState::Pressed, .. }, .. } => {
                let bytes: Vec<u8> = match &logical_key {
                    Key::Named(NamedKey::Enter) => vec![b'\n'],
                    Key::Named(NamedKey::Backspace) => vec![8],
                    Key::Named(NamedKey::Tab) => vec![b'\t'],
                    Key::Named(NamedKey::Space) => vec![b' '],
                    Key::Named(NamedKey::Escape) => vec![27],
                    Key::Character(sm) => sm.as_str().as_bytes().to_vec(),
                    _ => Vec::new(),
                };
                if !bytes.is_empty() {
                    let mut canvas_line: Option<String> = None;
                    if let Some(ws) = self.wins.get_mut(&wid) {
                        if let (Some(snap), Some(eid)) = (ws.cv.as_mut(), ws.cv_edit) {
                            if let Some(node) = snap.iter_mut().find(|n| n.id == eid) {
                                if bytes == [8] {
                                    // pop the last character (UTF-8 aware)
                                    let mut cut = node.data.len();
                                    while cut > 0 {
                                        cut -= 1;
                                        if node.data[cut] & 0xC0 != 0x80 { break; }
                                    }
                                    if cut < node.data.len() {
                                        let q1 = node.data.len();
                                        node.data.truncate(cut);
                                        canvas_line = Some(format!("delete {} {} {}", eid, cut, q1));
                                    }
                                } else {
                                    let at = node.data.len();
                                    let txt = String::from_utf8_lossy(&bytes).into_owned();
                                    node.data.extend_from_slice(&bytes); // presenter-local echo
                                    canvas_line = Some(format!("insert {} {} {}", eid, at, cv_q(&txt)));
                                }
                            }
                        } else if ws.has_text {
                            // local echo into the glass tty, so typing is visible
                            ws.tty.feed(&bytes);
                        }
                    }
                    if let Some(line) = canvas_line {
                        self.paint(wid);
                        let _ = self.ev.send(Ev::WinCanvasEv { wid, line });
                        return;
                    }
                    if self.wins.get(&wid).map(|w| w.has_text).unwrap_or(false) {
                        self.paint(wid);
                    }
                    let _ = self.ev.send(Ev::WinKey { wid, bytes });
                }
            }
            _ => {}
        }
    }
}

pub fn run(ev: Sender<Ev>, rootdir: &str) -> (EventLoop<UiMsg>, App) {
    let font = Subfont::load(&std::path::Path::new(rootdir).join("lib/font/bit/fixed/9x18.0000"));
    let el = EventLoop::<UiMsg>::with_user_event().build().expect("event loop");
    let app = App::new(ev, font);
    (el, app)
}
