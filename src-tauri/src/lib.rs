mod bridge;
mod chores;
mod ears;
mod hunt;
mod memory;

use bridge::creds::Creds;
use bridge::{detect, session, TurnRequest};
use chores::{Board, Chore, Draft, Gift};
use ears::Ears;
use hunt::Hunts;
use memory::{CatMemory, Memory};
use std::sync::Arc;
use tauri::{Manager, PhysicalPosition, PhysicalSize, State};

// Everything about where the cat stands is in *physical* pixels on the virtual
// desktop — the one coordinate space every monitor shares. Logical pixels would
// be shorter to write, but they're only meaningful relative to one monitor's
// scale factor, and a 150% laptop next to a 100% external has two of those.

/// Cat window geometry, mirrored from `tauri.conf.json`'s `main` window. The
/// cat is drawn in a box this size at the bottom-right of its window, which is
/// larger than the box only while the chat panel is open.
///
/// It is bigger than the cat: the extra is empty desktop for the yarn to roll
/// across and the hearts to drift into, so neither ends against a window edge.
/// It has to match `SCENE_W`/`SCENE_H` x `SCALE` from the frontend.
const CAT_SIZE: (f64, f64) = (240.0, 200.0);

/// One monitor's slice of the desktop, as far as the cat is concerned.
///
/// It's derived from the monitor's *work area* — the screen minus the taskbar
/// — so `floor` is the window y at which the cat's feet rest on that monitor's
/// taskbar. The window is always sized so the cat is drawn along its bottom
/// edge, which is what makes pinning the bottom edge equivalent to standing on
/// the ground.
#[derive(Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Screen {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    /// Window y with the cat's feet on this monitor's taskbar.
    floor: f64,
    /// Highest the window may be lifted here, i.e. the top of the work area.
    ceiling: f64,
    /// What the cat window measures on this monitor — it grows and shrinks
    /// with the DPI as it's carried between screens.
    win_w: f64,
    win_h: f64,
    /// Width of the cat itself here, which is less than `win_w` whenever the
    /// chat panel is open.
    cat_w: f64,
    scale: f64,
    /// Index into `Perch::strips` of the run this monitor belongs to.
    strip: usize,
}

/// A run of monitors whose work areas touch, so the cat can walk from one to
/// the next without stepping into the void. Monitors stacked one above the
/// other, or separated by a gap in the desktop, end up in different strips.
#[derive(Clone, Copy, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Strip {
    left: f64,
    right: f64,
}

/// Everywhere the cat may stand, and where it is standing now.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Perch {
    screens: Vec<Screen>,
    strips: Vec<Strip>,
    /// Index into `screens` of the monitor under the cat's feet.
    current: usize,
    x: f64,
    y: f64,
}

impl Perch {
    /// The monitor the cat is on.
    fn here(&self) -> Screen {
        self.screens[self.current]
    }

    /// Horizontal limits for a window `w` wide standing on `s`. The cat may
    /// straddle the seam between touching monitors, so these span the strip
    /// rather than the single screen.
    fn span(&self, s: &Screen, w: f64) -> (f64, f64) {
        let strip = self.strips[s.strip];
        (strip.left, (strip.right - w).max(strip.left))
    }
}

/// A monitor's work area, straight off the OS.
#[derive(Clone, Copy)]
struct Area {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    scale: f64,
}

/// Slack when deciding whether two work areas touch. Flush monitors share an
/// edge exactly, but a taskbar down the side of one can leave a sliver.
const SEAM: f64 = 2.0;

/// Groups touching work areas, returning each area's strip index.
fn strips_of(areas: &[Area]) -> Vec<usize> {
    let n = areas.len();
    let mut group: Vec<usize> = (0..n).collect();
    // Nobody has thirty monitors, so relabelling in place beats a union-find.
    for i in 0..n {
        for j in 0..i {
            let (a, b) = (areas[i], areas[j]);
            let touching = a.left <= b.right + SEAM && b.left <= a.right + SEAM;
            let alongside = a.top < b.bottom && b.top < a.bottom;
            if !(touching && alongside) {
                continue;
            }
            let (from, to) = (group[i].max(group[j]), group[i].min(group[j]));
            for g in group.iter_mut() {
                if *g == from {
                    *g = to;
                }
            }
        }
    }

    // Renumber to a dense 0..k so the labels can index into `strips`.
    let mut seen: Vec<usize> = Vec::new();
    group
        .iter()
        .map(|g| match seen.iter().position(|s| s == g) {
            Some(k) => k,
            None => {
                seen.push(*g);
                seen.len() - 1
            }
        })
        .collect()
}

/// Index of the screen under `(x, y)`, or the nearest one if it falls in a gap
/// between monitors.
///
/// Ranked horizontally first: a cat that has just stepped over a seam belongs
/// to the monitor it stepped onto, even though it's still standing at the old
/// taskbar's height and hasn't dropped to the new one yet. The vertical
/// distance only breaks ties, which is what separates stacked monitors.
fn screen_under(screens: &[Screen], x: f64, y: f64) -> usize {
    let mut best = 0;
    let mut best_gap = (f64::MAX, f64::MAX);
    for (i, s) in screens.iter().enumerate() {
        let gap = (
            (s.left - x).max(x - s.right).max(0.0),
            (s.top - y).max(y - s.bottom).max(0.0),
        );
        if gap < best_gap {
            best_gap = gap;
            best = i;
        }
    }
    best
}

/// `win` is the window's current physical rect; `logical_win` its size in
/// logical pixels, which is what stays constant as it crosses monitors.
fn perch_from(areas: &[Area], logical_win: (f64, f64), win: (f64, f64, f64, f64)) -> Option<Perch> {
    if areas.is_empty() {
        return None;
    }

    let labels = strips_of(areas);
    let mut strips = vec![
        Strip {
            left: f64::MAX,
            right: f64::MIN,
        };
        labels.iter().copied().max().map_or(0, |m| m + 1)
    ];
    for (a, &s) in areas.iter().zip(&labels) {
        strips[s].left = strips[s].left.min(a.left);
        strips[s].right = strips[s].right.max(a.right);
    }

    let screens: Vec<Screen> = areas
        .iter()
        .zip(&labels)
        .map(|(a, &strip)| {
            let win_w = logical_win.0 * a.scale;
            let win_h = logical_win.1 * a.scale;
            let floor = a.bottom - win_h;
            Screen {
                left: a.left,
                top: a.top,
                right: a.right,
                bottom: a.bottom,
                floor,
                // a window taller than the work area would invert these
                ceiling: a.top.min(floor),
                win_w,
                win_h,
                cat_w: CAT_SIZE.0 * a.scale,
                scale: a.scale,
                strip,
            }
        })
        .collect();

    // The cat stands at the bottom of its window, tucked into the right-hand
    // corner, and it's the cat's feet that decide which monitor it's on — not
    // the middle of a chat panel it happens to have open.
    let scale = if logical_win.0 > 0.0 {
        win.2 / logical_win.0
    } else {
        1.0
    };
    let cat_w = (CAT_SIZE.0 * scale).min(win.2);
    let current = screen_under(&screens, win.0 + win.2 - cat_w / 2.0, win.1 + win.3);
    Some(Perch {
        screens,
        strips,
        current,
        x: win.0,
        y: win.1,
    })
}

/// Reads every monitor's work area, plus where the window sits among them.
fn read_perch(window: &tauri::Window) -> Result<Perch, String> {
    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    let pos = window.outer_position().map_err(|e| e.to_string())?;

    let mut monitors = window.available_monitors().map_err(|e| e.to_string())?;
    if monitors.is_empty() {
        // a locked session can report nothing — take whatever we can still get
        monitors = window
            .current_monitor()
            .map_err(|e| e.to_string())?
            .or(window.primary_monitor().map_err(|e| e.to_string())?)
            .into_iter()
            .collect();
    }

    let areas: Vec<Area> = monitors
        .iter()
        .map(|m| {
            let work = m.work_area();
            Area {
                left: work.position.x as f64,
                top: work.position.y as f64,
                right: work.position.x as f64 + work.size.width as f64,
                bottom: work.position.y as f64 + work.size.height as f64,
                scale: m.scale_factor(),
            }
        })
        .collect();

    perch_from(
        &areas,
        (size.width as f64 / scale, size.height as f64 / scale),
        (
            pos.x as f64,
            pos.y as f64,
            size.width as f64,
            size.height as f64,
        ),
    )
    .ok_or_else(|| "no monitor available".to_string())
}

/// Where a cat should be put down when it opens.
///
/// `at` is the spot it remembers standing on last time. The desktop may not be
/// the one it left — a monitor unplugged, a taskbar moved, a laptop docked — so
/// the remembered x is pulled back onto whatever strip of screen is nearest,
/// and the y always comes from that monitor's taskbar rather than from memory.
/// A cat with nothing to remember starts over towards the right-hand end.
fn resting_place(p: &Perch, at: Option<(f64, f64)>) -> (f64, f64) {
    let here = p.here();
    match at {
        Some((x, y)) => {
            let foot_x = x + here.win_w - here.cat_w / 2.0;
            let s = p.screens[screen_under(&p.screens, foot_x, y + here.win_h)];
            let (min_x, max_x) = p.span(&s, s.win_w);
            (x.clamp(min_x, max_x), s.floor)
        }
        None => {
            let (min_x, max_x) = p.span(&here, here.win_w);
            ((max_x - 40.0 * here.scale).clamp(min_x, max_x), here.floor)
        }
    }
}

/// Stands a cat on the taskbar, where it left off if it remembers.
fn stand(window: &tauri::Window, at: Option<(f64, f64)>) {
    let Ok(p) = read_perch(window) else { return };
    let (x, y) = resting_place(&p, at);
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

/// Where the cat may stand, and where it is standing now.
#[tauri::command]
fn perch(window: tauri::Window) -> Result<Perch, String> {
    read_perch(&window)
}

/// What this cat remembers of its life so far.
#[tauri::command]
fn recall(window: tauri::Window, memory: State<'_, Arc<Memory>>) -> CatMemory {
    memory.recall(window.label())
}

/// Commits part of that memory. Patches are shallow and per-field, so the
/// animation loop and the chat can each write their own without reading the
/// other's back first.
#[tauri::command]
fn remember(
    window: tauri::Window,
    memory: State<'_, Arc<Memory>>,
    patch: serde_json::Value,
) -> Result<(), String> {
    memory.remember(window.label(), &patch)
}

/// Moves the window. The frontend runs the physics and clamps against the
/// perch it last read, so this is deliberately a dumb setter.
#[tauri::command]
fn hop(window: tauri::Window, x: f64, y: f64) -> Result<(), String> {
    window
        .set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32))
        .map_err(|e| e.to_string())
}

/// Agent CLIs the user has installed, so the UI can offer them as brains —
/// each one annotated with how the user has chosen to pay for it.
#[tauri::command]
fn bridge_detect(creds: State<'_, Arc<Creds>>) -> Vec<detect::Backend> {
    let mut backends = detect::detect_all();
    for backend in &mut backends {
        // Only backends that can take a key get asked about one; the rest stay
        // on the default, which is "whatever the CLI already does".
        if backend.key_env.is_none() {
            continue;
        }
        let status = creds.status(&backend.id);
        backend.auth = status.auth;
        backend.has_key = status.has_key;
    }
    backends
}

/// Saves an API key for a backend and switches it onto that key.
#[tauri::command]
fn creds_set_key(creds: State<'_, Arc<Creds>>, backend: String, key: String) -> Result<(), String> {
    creds.set_key(&backend, &key)
}

/// Goes back to spending the CLI's own login, dropping any saved key.
#[tauri::command]
fn creds_use_subscription(creds: State<'_, Arc<Creds>>, backend: String) -> Result<(), String> {
    creds.use_subscription(&backend)
}

/// Forgets the choice altogether, back to inheriting the environment.
#[tauri::command]
fn creds_clear(creds: State<'_, Arc<Creds>>, backend: String) -> Result<(), String> {
    creds.clear(&backend)
}

/// The user's home, used as the agent's default working directory.
pub fn user_home() -> String {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_default()
}

#[tauri::command]
fn home_dir() -> String {
    user_home()
}

/// Runs one turn for the calling cat. Resolves when the turn ends; progress
/// arrives as events addressed to that cat's window only.
#[tauri::command]
async fn bridge_send(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: State<'_, Arc<session::BridgeState>>,
    creds: State<'_, Arc<Creds>>,
    request: TurnRequest,
) -> Result<(), String> {
    let cat = window.label().to_string();
    session::run(
        app,
        state.inner().clone(),
        creds.inner().clone(),
        cat,
        request,
        // Not a hunt: these events belong in the conversation the user is
        // having, on the channel the panel reads.
        None,
    )
    .await
    .map(|_| ())
}

#[tauri::command]
fn bridge_cancel(window: tauri::Window, state: State<'_, Arc<session::BridgeState>>) {
    state.cancel(window.label());
}

// --- the chore board ---
//
// Chores belong to the cat whose window asks, always — there is no command
// here that lets one cat read or rewrite another's board. That's not a
// permission check (nothing in Purrch is); it's what makes a colony a colony
// rather than one shared to-do list shown in several windows.

/// Everything on this cat's board.
#[tauri::command]
fn chores_list(window: tauri::Window, board: State<'_, Arc<Board>>) -> Vec<Chore> {
    board.for_cat(window.label())
}

/// Hands this cat a new standing job.
#[tauri::command]
fn chores_add(window: tauri::Window, board: State<'_, Arc<Board>>, draft: Draft) -> Chore {
    board.add(window.label(), draft)
}

#[tauri::command]
fn chores_update(
    board: State<'_, Arc<Board>>,
    id: String,
    patch: serde_json::Value,
) -> Result<Chore, String> {
    board.update(&id, &patch)
}

#[tauri::command]
fn chores_remove(board: State<'_, Arc<Board>>, id: String) {
    board.remove(&id);
}

/// Sends the cat after one now instead of waiting for its slot. It still
/// queues behind whatever the cat is already doing, including you.
#[tauri::command]
fn chores_run_now(
    app: tauri::AppHandle,
    window: tauri::Window,
    board: State<'_, Arc<Board>>,
    hunts: State<'_, Arc<Hunts>>,
    id: String,
) {
    board.nudge(&id);
    hunts.queue(window.label(), &id);
    hunt::pump(&app, window.label());
}

/// What this cat is out doing right now, for the check-in line. Comes from
/// Rust so a reloaded window still knows what it walked in on.
#[tauri::command]
fn hunt_status(window: tauri::Window, hunts: State<'_, Arc<Hunts>>) -> hunt::Status {
    hunts.status(window.label())
}

/// This cat's pile, newest first.
#[tauri::command]
fn gifts_list(window: tauri::Window, board: State<'_, Arc<Board>>) -> Vec<Gift> {
    board.gifts_for(window.label())
}

/// Marks gifts as looked at. An empty list means the whole pile.
#[tauri::command]
fn gifts_read(window: tauri::Window, board: State<'_, Arc<Board>>, ids: Vec<String>) {
    board.read(window.label(), &ids);
}

#[tauri::command]
fn gifts_clear(window: tauri::Window, board: State<'_, Arc<Board>>) {
    board.clear(window.label());
}

/// This cat says what it's called and whether it wants to be listened for.
///
/// The frontend drives this rather than Rust reading the memory store, because
/// a cat's ears are only open once the user has agreed to let the colony act
/// unasked — and that agreement lives with the rest of the frontend's state.
/// Sent on every change; the ear reopens the microphone only when the colony
/// genuinely sounds different.
#[tauri::command]
fn ears_tune(
    app: tauri::AppHandle,
    window: tauri::Window,
    ears: State<'_, Arc<Ears>>,
    name: String,
    listening: bool,
) {
    ears.tune(&app, window.label(), name, listening);
}

/// Splits dropped paths into folders and files, so the frontend can decide
/// whether a drop re-scopes the session or just names something to work on.
#[derive(serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct Dropped {
    dirs: Vec<String>,
    files: Vec<String>,
}

#[tauri::command]
fn classify_paths(paths: Vec<String>) -> Dropped {
    let mut out = Dropped::default();
    for path in paths {
        // A path we can't stat is reported as a file; the agent will produce a
        // better error about it than we can.
        if std::path::Path::new(&path).is_dir() {
            out.dirs.push(path);
        } else {
            out.files.push(path);
        }
    }
    out
}

/// Builds a cat window, hidden. The caller stands it somewhere and shows it —
/// a cat that appeared in the default corner and then hopped to its own spot
/// would look like a bug rather than like coming home.
fn build_cat(app: &tauri::AppHandle, label: &str) -> Result<tauri::WebviewWindow, String> {
    tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::App("index.html".into()))
        .title("Purrch")
        .inner_size(CAT_SIZE.0, CAT_SIZE.1)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .visible(false)
        .build()
        .map_err(|e| e.to_string())
}

/// Adds another cat to the colony. Each one is a separate window with its own
/// label, which is what keeps its agent session — and its memory — independent.
#[tauri::command]
async fn spawn_cat(app: tauri::AppHandle, from: tauri::Window) -> Result<String, String> {
    // Labels must be unique and stable for the window's lifetime; a counter
    // would collide after a dismiss, so derive from the highest existing. A
    // label that comes free again keeps its cat — see `Memory::leave`.
    let label = (1..)
        .map(|n| format!("cat-{n}"))
        .find(|candidate| app.get_webview_window(candidate).is_none())
        .ok_or("couldn't allocate a cat label")?;

    let built = build_cat(&app, &label)?;

    // Drop the newcomer beside its parent rather than on top of it, on the
    // same monitor — its own size there, since the parent may have its chat
    // panel open, and a physical pixel is worth less on a high-DPI screen.
    if let Ok(p) = read_perch(&from) {
        let s = p.here();
        let (w, h) = (CAT_SIZE.0 * s.scale, CAT_SIZE.1 * s.scale);
        let (min_x, max_x) = p.span(&s, w);
        let x = (p.x - w - 12.0 * s.scale).clamp(min_x, max_x);
        let _ = built.set_position(PhysicalPosition::new(
            x.round() as i32,
            (s.bottom - h).round() as i32,
        ));
    }
    let _ = built.show();
    Ok(label)
}

/// Sends a cat home: it stops coming back at launch. The last one standing
/// closes the app instead — that's quitting Purrch, not saying goodbye to the
/// cat, so it's back on the taskbar next time exactly as you left it.
#[tauri::command]
fn dismiss_cat(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: State<'_, Arc<session::BridgeState>>,
    memory: State<'_, Arc<Memory>>,
) -> Result<(), String> {
    state.cancel_and_forget(window.label());
    if app.webview_windows().len() <= 1 {
        app.exit(0);
        return Ok(());
    }
    memory.leave(window.label());
    window.close().map_err(|e| e.to_string())
}

/// Window size with the chat panel open. Everything the cat shows you — the
/// chat, the collar, the chore board, the gift pile — is sized to this.
const PANEL: (f64, f64) = (380.0, 460.0);

/// Resizes the window to `target` logical pixels, keeping the cat pinned to
/// the same spot on screen instead of letting it jump.
fn resize_around_cat(window: &tauri::Window, target: (f64, f64)) -> Result<(), String> {
    let old = window.outer_size().map_err(|e| e.to_string())?;
    // Which monitor the cat is on has to be settled before the window grows,
    // or a panel opening beside a screen edge would spill over the seam and
    // take its floor from the neighbour.
    let p = read_perch(window)?;
    let s = p.here();

    let new = PhysicalSize::new(
        (target.0 * s.scale) as u32,
        (target.1 * s.scale) as u32,
    );
    window.set_size(new).map_err(|e| e.to_string())?;

    // The cat is drawn at the bottom-right of the window, so the panel has to
    // grow up and to the left to leave it standing where it was — and it still
    // has to land on the taskbar, inside the work area.
    let (min_x, max_x) = p.span(&s, new.width as f64);
    let grew = new.width as f64 - old.width as f64;
    let x = (p.x - grew).clamp(min_x, max_x);
    let y = s.bottom - new.height as f64;
    window
        .set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32))
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Grows or shrinks the window when the chat panel opens, keeping the cat
/// pinned to the same spot on screen instead of letting it jump.
#[tauri::command]
fn set_panel(window: tauri::Window, open: bool) -> Result<(), String> {
    resize_around_cat(&window, if open { PANEL } else { CAT_SIZE })
}

/// The window size that shows a menu wanting `want` whole, given what the
/// window measures now and how much room there is between the taskbar and the
/// top of the work area.
///
/// Only ever grows: the chat panel may be open behind the menu and is wider
/// than anything the menu needs, so `now` is a floor and not a starting point.
/// `headroom` is the one thing that can override it — a window taller than the
/// work area would have to lift the cat off the taskbar to fit.
fn room_for_menu(now: (f64, f64), want: (f64, f64), headroom: f64) -> (f64, f64) {
    (
        now.0.max(want.0).max(CAT_SIZE.0),
        now.1.max(want.1).max(CAT_SIZE.1).min(headroom),
    )
}

/// Makes room for the right-click menu.
///
/// The window is what clips the menu, and its bottom edge is the taskbar — so
/// a menu taller than the window isn't scrolled or squeezed to fit, it's
/// simply cut off along the taskbar. The cat's own window is 200 logical
/// pixels tall and the menu has outgrown that, so the window has to borrow the
/// height for as long as the menu is up. `set_panel` is what puts it back.
#[tauri::command]
fn set_menu(window: tauri::Window, w: f64, h: f64) -> Result<(), String> {
    let p = read_perch(&window)?;
    let s = p.here();
    let now = window.outer_size().map_err(|e| e.to_string())?;

    let target = room_for_menu(
        (now.width as f64 / s.scale, now.height as f64 / s.scale),
        (w, h),
        (s.bottom - s.ceiling) / s.scale,
    );
    resize_around_cat(&window, target)
}

pub fn run() {
    tauri::Builder::default()
        .manage(Arc::new(session::BridgeState::default()))
        .manage(Arc::new(Hunts::default()))
        .invoke_handler(tauri::generate_handler![
            bridge_detect,
            bridge_send,
            bridge_cancel,
            ears_tune,
            chores_list,
            chores_add,
            chores_update,
            chores_remove,
            chores_run_now,
            hunt_status,
            gifts_list,
            gifts_read,
            gifts_clear,
            creds_set_key,
            creds_use_subscription,
            creds_clear,
            classify_paths,
            spawn_cat,
            dismiss_cat,
            set_panel,
            set_menu,
            perch,
            hop,
            home_dir,
            recall,
            remember
        ])
        .on_window_event(|window, event| {
            // A closed window's agent must not keep running against the
            // user's subscription with nothing left to display its output —
            // nor may its name still be answered for by an open microphone.
            if matches!(event, tauri::WindowEvent::Destroyed) {
                if let Some(state) = window.try_state::<Arc<session::BridgeState>>() {
                    state.cancel_and_forget(window.label());
                }
                if let Some(ears) = window.try_state::<Arc<Ears>>() {
                    ears.forget(window.app_handle(), window.label());
                }
                // Its chores stay on the board — a cat that comes back to that
                // slot comes back to its own life (see `Memory::leave`) — but
                // nothing may still be queued for a window that isn't there.
                if let Some(hunts) = window.try_state::<Arc<Hunts>>() {
                    hunts.forget(window.label());
                }
            }
        })
        .setup(|app| {
            // The colony's memory has to exist before any cat can ask what it
            // remembers, so it's the first thing set up.
            let dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let memory = Arc::new(Memory::load(&dir));
            app.manage(memory.clone());

            // How each backend is paid for. Same directory, separate file —
            // and the keys themselves live in the OS credential store, not in
            // either of them.
            app.manage(Arc::new(Creds::load(&dir)));

            // The colony's hearing. Its transcriber and language model are a
            // ~150 MB download fetched on first use, so they go in local app
            // data rather than beside the config — nobody wants that following
            // them onto another machine with a roaming profile.
            let hearing = app
                .path()
                .app_local_data_dir()
                .unwrap_or_else(|_| dir.clone())
                .join("hearing");
            app.manage(Arc::new(Ears::new(hearing)));

            // The chore board, and the clock that reads it. The clock is
            // started last, once every cat is back on the desktop: a chore
            // only fires for a cat that's actually standing there, so starting
            // it first would drop the first tick's worth of work on the floor.
            app.manage(Arc::new(Board::load(&dir)));

            // Put every cat back where it was standing. The main window is
            // created hidden from the config so it appears in its own spot
            // rather than sliding there from the default corner.
            if let Some(win) = app.get_webview_window("main") {
                let win = win.as_ref().window();
                stand(&win, memory.recall("main").at());
                let _ = win.show();
            }

            // Anyone else who was on the desktop when Purrch last closed comes
            // back too — a colony you have to rebuild by hand every morning
            // isn't your colony.
            let handle = app.handle().clone();
            for label in memory.present() {
                if label == "main" || handle.get_webview_window(&label).is_some() {
                    continue;
                }
                match build_cat(&handle, &label) {
                    Ok(cat) => {
                        stand(&cat.as_ref().window(), memory.recall(&label).at());
                        let _ = cat.show();
                    }
                    // One cat that won't come back must not take the rest of
                    // the colony — or the app — down with it.
                    Err(e) => eprintln!("couldn't bring {label} back: {e}"),
                }
            }

            hunt::start(handle);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running purrch");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 1920x1080 at 100%, taskbar 40px along the bottom, placed at `left`.
    fn screen_1080p(left: f64) -> Area {
        Area {
            left,
            top: 0.0,
            right: left + 1920.0,
            bottom: 1040.0,
            scale: 1.0,
        }
    }

    const CAT: (f64, f64) = CAT_SIZE;

    /// A cat window standing at `(x, y)` on a monitor of the given scale.
    fn at(x: f64, y: f64, scale: f64) -> (f64, f64, f64, f64) {
        (x, y, CAT.0 * scale, CAT.1 * scale)
    }

    /// A 1080p work area: 1040 down to 0, so 1040 logical pixels of headroom.
    const HEADROOM: f64 = 1040.0;

    #[test]
    fn a_menu_taller_than_the_cat_lifts_the_window_off_the_taskbar() {
        // The whole bug: ten items don't fit in the cat's own 200px window, so
        // the bottom of the menu used to be cut off along the taskbar.
        let (w, h) = room_for_menu(CAT, (196.0, 288.0), HEADROOM);
        assert_eq!(h, 288.0);
        // Nothing is gained by narrowing: the cat's scene needs its own width.
        assert_eq!(w, CAT.0);
    }

    #[test]
    fn a_menu_that_already_fits_leaves_the_window_alone() {
        assert_eq!(room_for_menu(CAT, (120.0, 90.0), HEADROOM), CAT);
    }

    #[test]
    fn the_open_chat_panel_is_never_shrunk_to_the_menu() {
        // The menu is narrower and shorter than the panel behind it, and
        // closing it must not be what resizes the panel.
        assert_eq!(room_for_menu(PANEL, (196.0, 288.0), HEADROOM), PANEL);
    }

    #[test]
    fn a_menu_taller_than_the_work_area_stops_at_the_ceiling() {
        // Rather than lifting the cat off the taskbar to fit; the frontend
        // scrolls the overflow instead.
        let (_, h) = room_for_menu(CAT, (196.0, 1200.0), HEADROOM);
        assert_eq!(h, HEADROOM);
    }

    #[test]
    fn single_monitor_floor_sits_on_the_taskbar() {
        let p = perch_from(&[screen_1080p(0.0)], CAT, at(100.0, 840.0, 1.0)).unwrap();
        let s = p.here();
        assert_eq!(s.floor, 1040.0 - CAT.1);
        assert_eq!(s.ceiling, 0.0);
        assert_eq!(p.span(&s, s.win_w), (0.0, 1920.0 - CAT.0));
    }

    #[test]
    fn side_by_side_monitors_share_one_walkable_strip() {
        let areas = [screen_1080p(0.0), screen_1080p(1920.0)];
        let p = perch_from(&areas, CAT, at(2000.0, 860.0, 1.0)).unwrap();
        // Standing on the right-hand monitor...
        assert_eq!(p.current, 1);
        // ...but free to walk the full width of both.
        assert_eq!(p.span(&p.here(), 200.0), (0.0, 3840.0 - 200.0));
        assert_eq!(p.strips.len(), 1);
    }

    #[test]
    fn stacked_monitors_are_separate_strips() {
        // Second screen directly above the first: no walking between them.
        let above = Area {
            left: 0.0,
            top: -1080.0,
            right: 1920.0,
            bottom: 0.0,
            scale: 1.0,
        };
        let p = perch_from(&[screen_1080p(0.0), above], CAT, at(0.0, 860.0, 1.0)).unwrap();
        assert_eq!(p.strips.len(), 2);
        assert_ne!(p.screens[0].strip, p.screens[1].strip);
    }

    #[test]
    fn each_monitor_keeps_its_own_taskbar_height() {
        // The second screen has no taskbar, so its floor is lower down.
        let bare = Area {
            left: 1920.0,
            top: 0.0,
            right: 3840.0,
            bottom: 1080.0,
            scale: 1.0,
        };
        let p = perch_from(&[screen_1080p(0.0), bare], CAT, at(0.0, 840.0, 1.0)).unwrap();
        assert_eq!(p.screens[0].floor, 1040.0 - CAT.1);
        assert_eq!(p.screens[1].floor, 1080.0 - CAT.1);
    }

    #[test]
    fn window_size_follows_the_monitor_scale() {
        // A 150% laptop to the left of a 100% external.
        let laptop = Area {
            left: 0.0,
            top: 0.0,
            right: 2560.0,
            bottom: 1380.0,
            scale: 1.5,
        };
        let p = perch_from(&[laptop, screen_1080p(2560.0)], CAT, at(0.0, 1080.0, 1.5)).unwrap();
        assert_eq!(p.screens[0].win_h, CAT.1 * 1.5); // logical height at 150%
        assert_eq!(p.screens[0].floor, 1380.0 - CAT.1 * 1.5);
        assert_eq!(p.screens[1].win_h, CAT.1);
        assert_eq!(p.screens[1].floor, 1040.0 - CAT.1);
    }

    #[test]
    fn a_cat_in_the_gap_lands_on_the_nearest_monitor() {
        // Monitors with a hole between them; the cat is dropped in the hole.
        let areas = [screen_1080p(0.0), screen_1080p(3000.0)];
        let p = perch_from(&areas, CAT, at(2500.0, 860.0, 1.0)).unwrap();
        assert_eq!(p.strips.len(), 2);
        assert_eq!(p.current, 1); // the cat at 2620 is nearer 3000 than 1920
    }

    #[test]
    fn an_open_chat_panel_follows_the_cat_and_not_its_own_middle() {
        let areas = [screen_1080p(0.0), screen_1080p(1920.0)];
        let panel = (380.0, 460.0);

        // Panel open, wholly on the left monitor: cat at 1760, middle at 1690.
        let p = perch_from(&areas, panel, (1500.0, 580.0, 380.0, 460.0)).unwrap();
        assert_eq!(p.current, 0);

        // Nudged along until the cat — bottom-right of the window, at 1960 —
        // is over the right monitor. Going by the window's middle (1890) would
        // still say left, and hang the cat off the wrong taskbar.
        let p = perch_from(&areas, panel, (1700.0, 580.0, 380.0, 460.0)).unwrap();
        assert_eq!(p.current, 1);
    }

    #[test]
    fn no_monitors_means_no_perch() {
        assert!(perch_from(&[], CAT, at(0.0, 0.0, 1.0)).is_none());
    }

    /// Window y with the cat's feet on the taskbar of `screen_1080p`.
    const FLOOR: f64 = 1040.0 - CAT.1;

    #[test]
    fn a_cat_with_no_past_starts_in_the_corner() {
        let p = perch_from(&[screen_1080p(0.0)], CAT, at(0.0, FLOOR, 1.0)).unwrap();
        let (x, y) = resting_place(&p, None);
        assert_eq!(y, FLOOR); // on the taskbar
        assert_eq!(x, 1920.0 - CAT.0 - 40.0);
    }

    #[test]
    fn a_remembered_spot_is_returned_to() {
        let areas = [screen_1080p(0.0), screen_1080p(1920.0)];
        // Opened on the left monitor, but it was last seen on the right one.
        let p = perch_from(&areas, CAT, at(100.0, FLOOR, 1.0)).unwrap();
        assert_eq!(resting_place(&p, Some((2400.0, FLOOR))), (2400.0, FLOOR));
    }

    #[test]
    fn the_floor_comes_from_the_monitor_and_not_from_memory() {
        // The second screen lost its taskbar since the cat was last there.
        let bare = Area {
            left: 1920.0,
            top: 0.0,
            right: 3840.0,
            bottom: 1080.0,
            scale: 1.0,
        };
        let p = perch_from(&[screen_1080p(0.0), bare], CAT, at(0.0, FLOOR, 1.0)).unwrap();
        // Remembered standing at the old taskbar height; put down on the new.
        assert_eq!(
            resting_place(&p, Some((2400.0, FLOOR))),
            (2400.0, 1080.0 - CAT.1)
        );
    }

    #[test]
    fn a_cat_whose_monitor_is_gone_comes_back_onto_the_desktop() {
        // Last seen far off to the right, on a screen that's been unplugged.
        let p = perch_from(&[screen_1080p(0.0)], CAT, at(0.0, FLOOR, 1.0)).unwrap();
        let (x, y) = resting_place(&p, Some((3000.0, 400.0)));
        assert_eq!((x, y), (1920.0 - CAT.0, FLOOR));
    }
}
