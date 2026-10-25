//! The dependency dialogs (`Views/Dependencies`): asking before a mod's dependencies are loaded
//! (`DependencyLoadWindow`), a mod's dependencies with subscribe / fix (`DependencyListDetailWindow`)
//! and asking before a mod's loaded dependencies are left behind (`DependencyUnloadWindow`).

use std::collections::{HashMap, HashSet};

use eframe::egui::{self, Align2, Color32, Id, Rect, Sense, UiBuilder, pos2, vec2};

use super::gfx::Gfx;
use super::widgets::{checkbox16, command_button, flat_button, flat_button_width};
use super::{icons, theme};
use crate::library::{Dependency, Library};
use crate::steam::{ItemDetails, Steam};

/// Workshop details that haven't arrived after this long are left out, like vanilla's lookups
/// that time out.
const DETAILS_WAIT: f64 = 5.0;

/// What the dialogs need besides the library.
pub struct DialogEnv<'a> {
    pub steam: Option<&'a Steam>,
    pub workshop: &'a HashMap<u64, ItemDetails>,
    pub authors: &'a HashMap<u64, String>,
    pub awaiting_items: &'a mut Vec<u64>,
    /// Folder sizes of installed mods, in bytes, once known.
    pub size_of: &'a dyn Fn(&std::path::Path) -> Option<u64>,
}

pub enum DependencyDialog {
    /// Loading `originators` (at `index` in the load order) needs dependencies that aren't loaded.
    Load {
        originators: Vec<String>,
        index: Option<usize>,
        unchecked: HashSet<String>,
        opened: std::time::Instant,
        queried: HashSet<u64>,
    },
    /// The dependencies of mod `id`.
    List {
        id: String,
        unchecked: HashSet<String>,
        opened: std::time::Instant,
        queried: HashSet<u64>,
    },
    /// Unloading `originators` leaves `dependencies` loaded.
    Unload {
        originators: Vec<String>,
        dependencies: Vec<String>,
        unchecked: HashSet<String>,
    },
}

impl DependencyDialog {
    pub fn load(originators: Vec<String>, index: Option<usize>) -> Self {
        DependencyDialog::Load {
            originators,
            index,
            unchecked: HashSet::new(),
            opened: std::time::Instant::now(),
            queried: HashSet::new(),
        }
    }

    pub fn list(id: String) -> Self {
        DependencyDialog::List {
            id,
            unchecked: HashSet::new(),
            opened: std::time::Instant::now(),
            queried: HashSet::new(),
        }
    }
}

/// Loads `originators` (`LoadWithDependencies2`): at once when every dependency is loaded,
/// otherwise through the Load dependencies dialog.
pub fn request_load(
    originators: Vec<String>,
    index: Option<usize>,
    lib: &mut Library,
    workshop: &HashMap<u64, ItemDetails>,
) -> Option<DependencyDialog> {
    let (chain, unknown) = lib.dependency_chain(&originators, workshop);
    if unknown.is_empty() && chain.iter().all(|d| d.loaded) {
        lib.load_exactly(&originators, index);
        return None;
    }
    Some(DependencyDialog::load(originators, index))
}

/// Unloads `originators` (`SolveOrphanedDependencies`): at once when they leave no loaded
/// dependency behind, otherwise through the Unload dialog.
pub fn request_unload(originators: Vec<String>, lib: &mut Library) -> Option<DependencyDialog> {
    let dependencies = lib.orphaned_dependencies(&originators);
    if dependencies.is_empty() {
        for id in &originators {
            lib.unload(id);
        }
        return None;
    }
    Some(DependencyDialog::Unload {
        originators,
        dependencies,
        unchecked: HashSet::new(),
    })
}

/// Draws the dialog over the window; `false` once it has closed.
pub fn show(
    dialog: &mut DependencyDialog,
    ui: &mut egui::Ui,
    gfx: &mut Gfx,
    lib: &mut Library,
    env: &mut DialogEnv,
) -> bool {
    let win = ui.max_rect();
    ui.interact(
        win,
        Id::unique("dependency-overlay"),
        Sense::click_and_drag(),
    );
    ui.painter().rect_filled(win, 0.0, theme::STRONG_PANEL);
    match dialog {
        DependencyDialog::Load {
            originators,
            index,
            unchecked,
            opened,
            queried,
        } => {
            let (chain, unknown) = lib.dependency_chain(originators, env.workshop);
            let waiting = query_unknown(&unknown, queried, env, opened.elapsed().as_secs_f64());
            let required: Vec<&Dependency> = chain.iter().filter(|d| !d.loaded).collect();
            if !waiting && required.is_empty() {
                lib.load_exactly(originators, *index);
                return false;
            }
            let (frame, close) = frame(ui, gfx, win, vec2(500.0, 600.0), "Load dependencies");
            let body = banner(ui, frame.body, env.steam.is_none());
            let mut y = body.top() + 10.0;
            y += label(
                ui,
                pos2(body.left() + 10.0, y),
                body.width() - 20.0,
                "These mods are required dependencies for mods that you have chosen to load.",
                theme::WHITE,
            ) + 10.0;
            let list = Rect::from_min_max(pos2(body.left(), y), body.max);
            if waiting {
                spinner_text(ui, list, "Loading...");
            } else {
                dependency_rows(
                    ui,
                    gfx,
                    list,
                    &required,
                    unchecked,
                    env,
                    lib,
                    |_| true,
                    false,
                );
            }
            let answer = footer(
                ui,
                frame.footer,
                &["Load selected mods", "Ignore dependencies", "Cancel"],
            );
            match answer {
                Some(0) if !waiting => {
                    let checked: Vec<&Dependency> = required
                        .iter()
                        .copied()
                        .filter(|d| !unchecked.contains(&d.id))
                        .collect();
                    subscribe(checked.iter().copied().filter(|d| !d.subscribed), env);
                    let mut ids = originators.clone();
                    ids.extend(checked.iter().map(|d| d.id.clone()));
                    lib.load_exactly(&ids, *index);
                    false
                }
                Some(1) => {
                    lib.load_exactly(originators, *index);
                    false
                }
                Some(2) => false,
                _ => !close,
            }
        }
        DependencyDialog::List {
            id,
            unchecked,
            opened,
            queried,
        } => {
            let parent_loaded = lib.is_loaded(id);
            let (chain, unknown) = lib.dependency_chain(std::slice::from_ref(id), env.workshop);
            let waiting = query_unknown(&unknown, queried, env, opened.elapsed().as_secs_f64());
            let (frame, close) = frame(ui, gfx, win, vec2(600.0, 600.0), "Dependencies");
            let body = banner(ui, frame.body, env.steam.is_none());
            if waiting && chain.is_empty() {
                spinner_text(ui, body, "Loading...");
                return !close;
            }
            if chain.is_empty() {
                ui.painter().text(
                    body.center(),
                    Align2::CENTER_CENTER,
                    "No dependencies.",
                    theme::body(theme::HEADER_SIZE),
                    theme::WHITE,
                );
                return !close;
            }
            let mut y = body.top() + 10.0;
            y += label(
                ui,
                pos2(body.left() + 10.0, y),
                body.width() - 20.0,
                "This mod requires some other mods to work correctly:",
                theme::SECONDARY_LABEL,
            ) + 10.0;
            let checkable = |d: &Dependency| !d.subscribed || (parent_loaded && !d.loaded);
            let fixable = chain.iter().any(checkable);
            // The fix panel: a red note and the Fix selected dependencies command.
            let mut list_bottom = body.bottom();
            let mut fix = None;
            if fixable {
                let width = body.width() - 20.0;
                let (_, height) = measure_command(ui, width);
                let panel_top = body.bottom() - 10.0 - height - 26.0;
                list_bottom = panel_top - 5.0;
                ui.painter().text(
                    pos2(body.left() + 10.0, panel_top),
                    Align2::LEFT_TOP,
                    "Some dependencies are needed.",
                    theme::body(theme::BODY_SIZE),
                    theme::ERROR,
                );
                fix = Some(
                    command_button(
                        ui,
                        gfx,
                        Id::unique("dependencies-fix"),
                        pos2(body.left() + 10.0, panel_top + 26.0),
                        width,
                        "Fix selected dependencies",
                        "This will load the selected mods, and missing mods will be automatically subscribed.",
                        false,
                    )
                    .0,
                );
            }
            let rows: Vec<&Dependency> = chain.iter().collect();
            dependency_rows(
                ui,
                gfx,
                Rect::from_min_max(pos2(body.left(), y), pos2(body.right(), list_bottom)),
                &rows,
                unchecked,
                env,
                lib,
                checkable,
                true,
            );
            if fix.is_some_and(|r| r.clicked()) {
                let selected: Vec<&Dependency> = chain
                    .iter()
                    .filter(|d| checkable(d) && !unchecked.contains(&d.id))
                    .collect();
                subscribe(selected.iter().copied().filter(|d| !d.subscribed), env);
                let ids: Vec<String> = selected.iter().map(|d| d.id.clone()).collect();
                lib.load_exactly(&ids, None);
                return false;
            }
            !close
        }
        DependencyDialog::Unload {
            originators,
            dependencies,
            unchecked,
        } => {
            let (frame, close) = frame(
                ui,
                gfx,
                win,
                vec2(500.0, 600.0),
                "Unload mod with dependencies",
            );
            let body = frame.body;
            let mut y = body.top() + 10.0;
            y += label(
                ui,
                pos2(body.left() + 10.0, y),
                body.width() - 20.0,
                "Unloading this mod will make the following dependencies useless.",
                theme::SECONDARY_LABEL,
            ) + 10.0;
            let width = body.width() - 20.0;
            let only = "Only the chosen mod will be unloaded. All listed dependent mods will remain loaded.";
            let selected = "The chosen mod will be unloaded including selected dependencies.";
            let (_, h1) = measure_command_text(ui, width, "Unload this mod only", only);
            let (_, h2) = measure_command_text(ui, width, "Unload selected", selected);
            let buttons_top = body.bottom() - 10.0 - h2 - 10.0 - h1;
            for (i, dep) in dependencies.iter().enumerate() {
                let row = Rect::from_min_size(
                    pos2(body.left() + 10.0, y + i as f32 * 30.0),
                    vec2(body.width() - 20.0, 30.0),
                );
                if row.bottom() > buttons_top - 5.0 {
                    break;
                }
                ui.painter().hline(
                    row.x_range(),
                    row.bottom() - 0.5,
                    (1.0, theme::argb(0xFF333333)),
                );
                let checked = !unchecked.contains(dep);
                if checkbox16(
                    ui,
                    gfx,
                    Id::unique(("unload-dependency", dep)),
                    pos2(row.left() + 8.0, row.center().y),
                    checked,
                )
                .clicked()
                {
                    toggle(unchecked, dep);
                }
                let name = lib.find(dep).map_or(dep.clone(), |m| m.name.clone());
                ui.painter().text(
                    pos2(row.left() + 26.0, row.center().y),
                    Align2::LEFT_CENTER,
                    name,
                    theme::body(theme::BODY_SIZE),
                    theme::WHITE,
                );
            }
            let (this_only, _) = command_button(
                ui,
                gfx,
                Id::unique("unload-only"),
                pos2(body.left() + 10.0, buttons_top),
                width,
                "Unload this mod only",
                only,
                true,
            );
            let (with_selected, _) = command_button(
                ui,
                gfx,
                Id::unique("unload-selected"),
                pos2(body.left() + 10.0, buttons_top + h1 + 10.0),
                width,
                "Unload selected",
                selected,
                false,
            );
            let cancel = footer(ui, frame.footer, &["Cancel"]).is_some();
            if this_only.clicked() || with_selected.clicked() {
                for id in originators.iter() {
                    lib.unload(id);
                }
                if with_selected.clicked() {
                    for dep in dependencies.iter().filter(|d| !unchecked.contains(*d)) {
                        lib.unload(dep);
                    }
                }
                return false;
            }
            !(cancel || close)
        }
    }
}

/// Asks Steam for the details of dependencies it hasn't described yet; `true` while some are
/// still awaited.
fn query_unknown(
    unknown: &[u64],
    queried: &mut HashSet<u64>,
    env: &DialogEnv,
    waited: f64,
) -> bool {
    let Some(steam) = env.steam else {
        return false;
    };
    let new: Vec<u64> = unknown
        .iter()
        .copied()
        .filter(|id| queried.insert(*id))
        .collect();
    if !new.is_empty() {
        steam.query_details(&new);
    }
    !unknown.is_empty() && waited < DETAILS_WAIT
}

/// Subscribes to the dependencies and polls Steam until it has installed them.
fn subscribe<'a>(deps: impl Iterator<Item = &'a Dependency>, env: &mut DialogEnv) {
    let Some(steam) = env.steam else { return };
    let ids: Vec<u64> = deps.map(|d| d.workshop_id).collect();
    for id in &ids {
        steam.subscribe(*id);
        if !env.awaiting_items.contains(id) {
            env.awaiting_items.push(*id);
        }
    }
    if !ids.is_empty() {
        steam.query_details(&ids);
    }
}

fn toggle(set: &mut HashSet<String>, id: &str) {
    if !set.remove(id) {
        set.insert(id.to_owned());
    }
}

struct Frame {
    body: Rect,
    footer: Rect,
}

/// `ModernCompositeDialogStyle`: `#111111` with a `#222222` border, a 35 px title bar with only a
/// close button, the body, and a footer row for the buttons. Returns the parts and whether the
/// close button was clicked.
fn frame(
    ui: &mut egui::Ui,
    gfx: &mut Gfx,
    win: Rect,
    size: egui::Vec2,
    title: &str,
) -> (Frame, bool) {
    let size = size.min(win.size() - vec2(40.0, 40.0));
    let rect = Rect::from_center_size(win.center(), size);
    ui.painter().rect_filled(rect, 0.0, theme::argb(0xFF111111));
    ui.painter().rect_stroke(
        rect,
        0.0,
        (1.0, theme::argb(0xFF222222)),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        pos2(rect.left() + 10.0, rect.top() + 18.0),
        Align2::LEFT_CENTER,
        title,
        theme::body(theme::HEADER_SIZE),
        theme::WHITE,
    );
    let close = Rect::from_min_size(pos2(rect.right() - 46.0, rect.top()), vec2(46.0, 35.0));
    let response = ui.interact(close, Id::unique(("dialog-close", title)), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(close, 0.0, theme::CAPTION_HOVER);
    }
    gfx.paint_icon(ui, icons::EXIT, close, vec2(12.0, 10.0), theme::WHITE);
    let footer = Rect::from_min_max(pos2(rect.left(), rect.bottom() - 50.0), rect.max);
    let body = Rect::from_min_max(
        pos2(rect.left(), rect.top() + 35.0),
        pos2(rect.right(), footer.top()),
    );
    (Frame { body, footer }, response.clicked())
}

/// The "Steam client is not online" banner (`#800000`) at the top of the body when offline;
/// returns the rest of the body.
fn banner(ui: &egui::Ui, body: Rect, offline: bool) -> Rect {
    if !offline {
        return body;
    }
    let bar = Rect::from_min_size(body.min, vec2(body.width(), 35.0));
    ui.painter().rect_filled(bar, 0.0, theme::argb(0xFF800000));
    ui.painter().text(
        pos2(bar.left() + 10.0, bar.center().y),
        Align2::LEFT_CENTER,
        "Steam client is not online. Information about dependencies might not be accurate.",
        theme::body(theme::BODY_SIZE),
        theme::WHITE,
    );
    Rect::from_min_max(pos2(body.left(), bar.bottom()), body.max)
}

/// Wrapped 14 px text; returns its height.
fn label(ui: &egui::Ui, pos: egui::Pos2, width: f32, text: &str, color: Color32) -> f32 {
    let galley = ui
        .painter()
        .layout(text.to_owned(), theme::body(theme::BODY_SIZE), color, width);
    let height = galley.size().y;
    ui.painter().galley(pos, galley, color);
    height
}

fn spinner_text(ui: &egui::Ui, rect: Rect, text: &str) {
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        theme::body(theme::HEADER_SIZE),
        theme::WHITE,
    );
    ui.ctx().request_repaint();
}

/// Footer buttons, right-aligned in the given order; the first is the default (accent).
fn footer(ui: &mut egui::Ui, rect: Rect, labels: &[&str]) -> Option<usize> {
    let mut right = rect.right() - 10.0;
    let mut clicked = None;
    for (i, label) in labels.iter().enumerate().rev() {
        let w = flat_button_width(ui, label).max(90.0);
        let b = Rect::from_min_max(
            pos2(right - w, rect.top() + 10.0),
            pos2(right, rect.bottom() - 10.0),
        );
        right -= w + 10.0;
        let response = if i == 0 && labels.len() > 1 {
            ui.painter().rect_filled(b, 0.0, theme::accent());
            let r =
                super::widgets::overlay_button(ui, Id::unique(("dependency-footer", *label)), b);
            ui.painter().text(
                b.center(),
                Align2::CENTER_CENTER,
                *label,
                theme::body(theme::BODY_SIZE),
                theme::WHITE,
            );
            r
        } else {
            flat_button(ui, Id::unique(("dependency-footer", *label)), b, label)
        };
        if response.clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

fn measure_command(ui: &egui::Ui, width: f32) -> (f32, f32) {
    measure_command_text(
        ui,
        width,
        "Fix selected dependencies",
        "This will load the selected mods, and missing mods will be automatically subscribed.",
    )
}

/// The size `command_button` will take.
fn measure_command_text(ui: &egui::Ui, width: f32, title: &str, description: &str) -> (f32, f32) {
    let text_width = (width - 60.0).max(50.0);
    let t = ui.painter().layout(
        title.to_uppercase(),
        theme::heading(17.0),
        Color32::WHITE,
        text_width,
    );
    let d = ui.painter().layout(
        description.to_owned(),
        theme::body(theme::BODY_SIZE),
        Color32::WHITE,
        text_width,
    );
    (width, (20.0 + t.size().y + d.size().y).max(30.0))
}

/// The dependency list (`DependencyItemViewModel` rows): checkbox, name, size and author, the
/// state line (`with_state`), and Subscribe / More info for missing mods.
#[allow(clippy::too_many_arguments)]
fn dependency_rows(
    ui: &mut egui::Ui,
    gfx: &mut Gfx,
    rect: Rect,
    deps: &[&Dependency],
    unchecked: &mut HashSet<String>,
    env: &mut DialogEnv,
    lib: &Library,
    checkable: impl Fn(&Dependency) -> bool,
    with_state: bool,
) {
    let row_height = if with_state { 70.0 } else { 50.0 };
    let mut list_ui = ui.new_child(UiBuilder::new().max_rect(rect));
    list_ui.set_clip_rect(rect.intersect(ui.clip_rect()));
    let mut subscribe_now = Vec::new();
    egui::ScrollArea::vertical()
        .id_salt(("dependency-rows", with_state))
        .auto_shrink([false, false])
        .show(&mut list_ui, |ui| {
            for dep in deps {
                let (row, _) =
                    ui.allocate_exact_size(vec2(rect.width(), row_height), Sense::hover());
                let inner = row.shrink2(vec2(10.0, 5.0));
                ui.painter().hline(
                    row.x_range(),
                    row.bottom() - 0.5,
                    (1.0, theme::argb(0xFF333333)),
                );
                if checkable(dep) {
                    let checked = !unchecked.contains(&dep.id);
                    if checkbox16(
                        ui,
                        gfx,
                        Id::unique(("dependency-check", &dep.id, with_state)),
                        pos2(inner.left() + 8.0, inner.top() + 10.0),
                        checked,
                    )
                    .clicked()
                    {
                        toggle(unchecked, &dep.id);
                    }
                }
                // Subscribe, then More info (the Workshop page) at the right, centred on the
                // row; the text column is clipped before them.
                let buttons = (!dep.subscribed).then(|| {
                    let info_w = 16.0 + 6.0 + flat_button_width(ui, "More info");
                    let info = Rect::from_min_size(
                        pos2(inner.right() - info_w, row.center().y - 16.0),
                        vec2(info_w, 32.0),
                    );
                    let sub_w = 40.0 + flat_button_width(ui, "Subscribe");
                    let sub = Rect::from_min_size(
                        pos2(info.left() - 10.0 - sub_w, info.top()),
                        vec2(sub_w, 32.0),
                    );
                    (sub, info)
                });
                let text_right = buttons.map_or(inner.right(), |(sub, _)| sub.left() - 5.0);
                let text = ui.painter().with_clip_rect(
                    Rect::from_min_max(row.min, pos2(text_right, row.bottom()))
                        .intersect(ui.clip_rect()),
                );
                let left = inner.left() + 30.0;
                let font = theme::body(theme::BODY_SIZE);
                text.text(
                    pos2(left, inner.top() + 10.0),
                    Align2::LEFT_CENTER,
                    &dep.name,
                    font.clone(),
                    theme::WHITE,
                );
                let details = env.workshop.get(&dep.workshop_id);
                let local = lib.find(&dep.id);
                let size = details
                    .map(|d| d.size)
                    .or_else(|| local.and_then(|m| (env.size_of)(&m.path)))
                    .map(|b| b as f64 / 1_048_576.0)
                    .unwrap_or(0.0);
                let author = local
                    .map(|m| m.author.clone())
                    .filter(|a| !a.is_empty())
                    .or_else(|| details.and_then(|d| env.authors.get(&d.owner).cloned()))
                    .unwrap_or_default();
                text.text(
                    pos2(left, inner.top() + 30.0),
                    Align2::LEFT_CENTER,
                    format!("Size: {size:.1} MB     Author: {author}"),
                    font.clone(),
                    theme::DIM_NAME,
                );
                if with_state {
                    let (state, color) = match (dep.subscribed, dep.loaded) {
                        (false, _) => ("Not subscribed", theme::ERROR),
                        (true, false) if checkable(dep) => ("Subscribed, not loaded", theme::ERROR),
                        (true, false) => ("Subscribed", theme::SECONDARY_GREEN),
                        (true, true) => ("Subscribed and loaded", theme::SECONDARY_GREEN),
                    };
                    text.text(
                        pos2(left, inner.top() + 50.0),
                        Align2::LEFT_CENTER,
                        state,
                        font.clone(),
                        color,
                    );
                }
                if let Some((sub, info)) = buttons {
                    if flat_button(ui, Id::unique(("dependency-info", &dep.id)), info, "").clicked()
                    {
                        let _ = open::that_detached(format!(
                            "https://steamcommunity.com/sharedfiles/filedetails/?id={}",
                            dep.workshop_id
                        ));
                    }
                    gfx.paint_icon(
                        ui,
                        icons::OPEN_EXTERNAL,
                        Rect::from_min_size(info.min + vec2(10.0, 9.0), vec2(14.0, 14.0)),
                        vec2(14.0, 14.0),
                        theme::WHITE,
                    );
                    ui.painter().text(
                        pos2(info.left() + 30.0, info.center().y),
                        Align2::LEFT_CENTER,
                        "More info",
                        font.clone(),
                        theme::WHITE,
                    );
                    let response =
                        flat_button(ui, Id::unique(("dependency-sub", &dep.id)), sub, "");
                    gfx.paint_icon(
                        ui,
                        icons::STEAM_ICON,
                        Rect::from_min_size(sub.min + vec2(8.0, 8.0), vec2(16.0, 16.0)),
                        vec2(16.0, 16.0),
                        theme::WHITE,
                    );
                    ui.painter().text(
                        pos2(sub.left() + 24.0, sub.top() + 10.0),
                        Align2::CENTER_CENTER,
                        "+",
                        theme::body(12.0),
                        theme::SECONDARY_GREEN,
                    );
                    ui.painter().text(
                        pos2(sub.left() + 34.0, sub.center().y),
                        Align2::LEFT_CENTER,
                        "Subscribe",
                        font.clone(),
                        theme::WHITE,
                    );
                    if response.clicked() && env.steam.is_some() {
                        subscribe_now.push((*dep).clone());
                    }
                }
            }
        });
    subscribe(subscribe_now.iter(), env);
}
