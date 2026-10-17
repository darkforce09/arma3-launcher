//! The Parameters page (`CC:Parameters` in `ParametersStyle.xaml`): Favorites and All
//! parameters tabs, grouped rows with a checkbox and an editor per parameter.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Align2, Id, Rect, Sense, UiBuilder, pos2, vec2};

use super::gfx::Gfx;
use super::widgets::{
    self, ToolbarIcon, browse_button, checkbox16, combo, filter_box, number_box, section_header,
    tab_control, text_box, toolbar,
};
use super::{icons, theme};
use crate::params::{self, DEFS, Def, GROUPS, Kind, Settings};
use crate::vanilla::wine_path;

const ROW_HEIGHT: f32 = 40.0;
const EDITOR_HEIGHT: f32 = 26.0;
const EDITOR_WIDTH: f32 = 300.0;

/// Lists the editors need, read once.
pub struct ParamsEnv<'a> {
    pub game_dir: &'a Path,
    pub profiles: &'a [String],
    pub allocators: &'a [String],
    pub total_memory: i64,
}

pub struct ParametersPage {
    tab: usize,
    filter: String,
    picked_tx: Sender<(&'static str, PathBuf, bool)>,
    picked_rx: Receiver<(&'static str, PathBuf, bool)>,
}

impl ParametersPage {
    pub fn new() -> Self {
        let (picked_tx, picked_rx) = channel();
        Self {
            tab: 0,
            filter: String::new(),
            picked_tx,
            picked_rx,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        page: Rect,
        settings: &mut Settings,
        favorites: &mut Vec<String>,
        env: &ParamsEnv,
    ) {
        // File and folder dialog results: set the value (folder lists append) and enable it.
        while let Ok((value_setting, path, append)) = self.picked_rx.try_recv() {
            let def = DEFS.iter().find(|d| d.value == value_setting);
            let mut value = wine_path(&path);
            if append {
                let current = settings.get(value_setting);
                if !current.trim().is_empty() {
                    value = format!("{};{value}", current.trim_end_matches(';'));
                }
            }
            settings.set(value_setting, value);
            if let Some(def) = def {
                settings.set_flag(def.enabled, true);
            }
        }

        let (tab, content) = tab_control(
            ui,
            "parameters-tabs",
            page,
            &["Favorites", "All parameters"],
            self.tab,
        );
        self.tab = tab;
        let bar = Rect::from_min_size(content.min, vec2(content.width(), 40.0));
        toolbar(ui, bar);
        let mut x = bar.left();
        let restore = widgets::toolbar_button(
            ui,
            gfx,
            Id::unique("parameters-restore"),
            &mut x,
            bar,
            ToolbarIcon::Glyph(icons::UNDO),
            "Restore defaults",
        );
        if restore.clicked() {
            // "Reset to default" turns every parameter off and keeps the values.
            for def in DEFS.iter().filter(|d| !d.enabled.is_empty() && !d.always) {
                settings.set_flag(def.enabled, false);
            }
        }
        if self.tab == 1 {
            let width = 300.0f32
                .min(bar.width() - (x - bar.left()) - 6.0)
                .max(200.0);
            let filter_rect = Rect::from_min_size(
                pos2(bar.right() - 3.0 - width, bar.top() + 4.0),
                vec2(width, 32.0),
            );
            filter_box(
                ui,
                gfx,
                Id::unique("parameters-filter"),
                filter_rect,
                &mut self.filter,
                "Search parameters (Ctrl+F)",
            );
        }

        let body = Rect::from_min_max(pos2(content.left(), bar.bottom() + 1.0), content.max);
        ui.painter().rect_filled(body, 0.0, theme::PANEL);
        let mut body_ui = ui.new_child(UiBuilder::new().max_rect(body));
        body_ui.set_clip_rect(body.intersect(ui.clip_rect()));
        let title_width = DEFS
            .iter()
            .map(|d| {
                ui.painter()
                    .layout_no_wrap(
                        format!("{}:", d.label),
                        theme::body(theme::BODY_SIZE),
                        theme::WHITE,
                    )
                    .size()
                    .x
                    + 10.0
            })
            .fold(140.0f32, f32::max)
            .min(240.0);
        let filter = self.filter.to_lowercase();
        let tab = self.tab;
        let visible = |d: &Def| {
            !d.hidden()
                && if tab == 0 {
                    favorites.iter().any(|f| f.eq_ignore_ascii_case(d.key()))
                } else {
                    filter.is_empty()
                        || d.label.to_lowercase().contains(&filter)
                        || d.name.to_lowercase().contains(&filter)
                }
        };
        let rows: Vec<(&str, &str, Vec<&'static Def>)> = GROUPS
            .iter()
            .map(|(id, label)| {
                (
                    *id,
                    *label,
                    DEFS.iter()
                        .filter(|d| d.group == *id && visible(d))
                        .collect::<Vec<_>>(),
                )
            })
            .filter(|(_, _, defs)| !defs.is_empty())
            .collect();

        let mut favorite_toggle: Option<&'static str> = None;
        egui::ScrollArea::vertical()
            .id_salt(("parameters-list", tab))
            .auto_shrink([false, false])
            .show(&mut body_ui, |ui| {
                let width = body.width();
                for (group, label, defs) in &rows {
                    let (header, _) = ui.allocate_exact_size(vec2(width, 50.0), Sense::hover());
                    section_header(
                        ui,
                        Id::unique(("parameter-group", group)),
                        header,
                        label,
                        (tab == 1).then_some(defs.len()),
                        None,
                    );
                    for def in defs {
                        ui.allocate_exact_size(vec2(width, 1.0), Sense::hover());
                        let (row, row_response) =
                            ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
                        if let Some(key) = self.row(
                            ui,
                            gfx,
                            row,
                            row_response.hovered(),
                            def,
                            title_width,
                            settings,
                            favorites,
                            env,
                        ) {
                            favorite_toggle = Some(key);
                        }
                    }
                }
            });
        if let Some(key) = favorite_toggle {
            match favorites.iter().position(|f| f.eq_ignore_ascii_case(key)) {
                Some(i) => {
                    favorites.remove(i);
                }
                None => favorites.push(key.to_owned()),
            }
        }
    }

    /// One parameter row (`ParameterItemContainerTemplate` + `TemplateParameterControl`).
    /// Returns the favourites key when its star was clicked.
    #[allow(clippy::too_many_arguments)]
    fn row(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        row: Rect,
        hovered: bool,
        def: &'static Def,
        title_width: f32,
        settings: &mut Settings,
        favorites: &[String],
        env: &ParamsEnv,
    ) -> Option<&'static str> {
        let mut toggled_favorite = None;
        ui.painter().rect_filled(
            row,
            0.0,
            if hovered {
                theme::argb(0xEE000000)
            } else {
                theme::PANEL
            },
        );
        let y = row.center().y;

        // Favourite star: invisible until the row is hovered, accent when its button is.
        let star = Rect::from_center_size(pos2(row.left() + 20.0, y), vec2(20.0, 20.0));
        let star_response = ui.interact(
            star,
            Id::unique(("parameter-star", def.key(), def.group)),
            Sense::click(),
        );
        let is_favorite = favorites.iter().any(|f| f.eq_ignore_ascii_case(def.key()));
        if hovered || star_response.hovered() {
            let (icon, tint) = if star_response.hovered() {
                (
                    if is_favorite {
                        icons::FAVORITE_STAR_REMOVE
                    } else {
                        icons::FAVORITE_STAR
                    },
                    theme::ACCENT,
                )
            } else {
                (icons::FAVORITE_STAR, theme::WHITE.gamma_multiply(0.8))
            };
            gfx.paint_icon(ui, icon, star, vec2(16.0, 16.0), tint);
        }
        if star_response.clicked() {
            toggled_favorite = Some(def.key());
        }

        // Title, right-aligned; clicking it toggles the parameter.
        let title_right = row.left() + 40.0 + title_width;
        let title = Rect::from_min_max(
            pos2(row.left() + 40.0, row.top()),
            pos2(title_right, row.bottom()),
        );
        let enabled = settings.enabled(def);
        let title_response = ui.interact(
            title,
            Id::unique(("parameter-title", def.key(), def.group)),
            Sense::click(),
        );
        ui.painter().text(
            pos2(title_right - 5.0, y),
            Align2::RIGHT_CENTER,
            format!("{}:", def.label),
            theme::body(theme::BODY_SIZE),
            theme::WHITE,
        );
        if !def.always && title_response.clicked() {
            settings.set_flag(def.enabled, !enabled);
        }
        let tooltip = |ui: &mut egui::Ui| {
            ui.set_max_width(400.0);
            if !def.name.is_empty() {
                ui.label(
                    egui::RichText::new(format!("Command line parameter: -{}", def.name))
                        .color(theme::SECONDARY_LABEL),
                );
                ui.add_space(10.0);
            }
            let description = def
                .description
                .replace("{0}", &env.total_memory.to_string());
            ui.label(egui::RichText::new(description).color(theme::WHITE));
        };
        title_response.on_hover_ui(tooltip);

        if !def.always {
            let check = checkbox16(
                ui,
                gfx,
                Id::unique(("parameter-check", def.key(), def.group)),
                pos2(title_right + 20.0, y),
                enabled,
            );
            if check.clicked() {
                settings.set_flag(def.enabled, !enabled);
            }
        }

        // The editor, at half opacity while the parameter is off.
        let editor = Rect::from_min_size(
            pos2(title_right + 40.0, y - EDITOR_HEIGHT / 2.0),
            vec2(
                EDITOR_WIDTH
                    .min(row.right() - title_right - 50.0)
                    .max(200.0),
                EDITOR_HEIGHT,
            ),
        );
        if def.value.is_empty() && !matches!(def.kind, Kind::DropDown) {
            return toggled_favorite;
        }
        let mut editor_ui = ui.new_child(UiBuilder::new().max_rect(editor.expand(2.0)));
        if !enabled {
            editor_ui.multiply_opacity(0.5);
        }
        let changed = self.editor(&mut editor_ui, gfx, editor, def, settings, env);
        if changed && !def.always && !def.enabled.is_empty() {
            settings.set_flag(def.enabled, true);
        }
        toggled_favorite
    }

    fn editor(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        rect: Rect,
        def: &'static Def,
        settings: &mut Settings,
        env: &ParamsEnv,
    ) -> bool {
        let id = Id::unique(("parameter-editor", def.key(), def.group));
        let setting = if def.name.is_empty() && def.id == "[Platform]" {
            "Platform"
        } else {
            def.value
        };
        let current = settings.get(setting);
        match def.kind {
            Kind::Basic => false,
            Kind::IntegerValue => {
                let max = if def.max == i64::MAX {
                    env.total_memory
                } else {
                    def.max
                };
                let min = if def.min == i64::MIN { 0 } else { def.min };
                let mut value = current.parse::<i64>().unwrap_or(min);
                if number_box(ui, gfx, id, rect, &mut value, min, max, def.step.max(1)) {
                    settings.set(setting, value.to_string());
                    true
                } else {
                    false
                }
            }
            Kind::ExtraThreads => {
                // File operations = 1, texture loading = 2, geometry loading = 4.
                let mask = current.parse::<u32>().unwrap_or(0);
                let mut new_mask = mask;
                let mut x = rect.left() + 8.0;
                for (bit, label) in [
                    (1, "File operations"),
                    (2, "Texture loading"),
                    (4, "Geometry loading"),
                ] {
                    let on = mask & bit != 0;
                    if checkbox16(ui, gfx, id.with(bit), pos2(x, rect.center().y), on).clicked() {
                        new_mask ^= bit;
                    }
                    let galley =
                        ui.painter()
                            .layout_no_wrap(label.into(), theme::body(12.0), theme::WHITE);
                    let w = galley.size().x;
                    ui.painter().galley(
                        pos2(x + 12.0, rect.center().y - galley.size().y / 2.0),
                        galley,
                        theme::WHITE,
                    );
                    x += 12.0 + w + 16.0;
                }
                if new_mask != mask {
                    settings.set(setting, new_mask.to_string());
                }
                new_mask != mask
            }
            Kind::DropDown | Kind::Combo => {
                let options: Vec<(String, String)> = match def.name {
                    "" if def.id == "[Platform]" => params::platforms(env.game_dir)
                        .into_iter()
                        .map(|(v, l)| (v.to_owned(), l.to_owned()))
                        .collect(),
                    "name" => env
                        .profiles
                        .iter()
                        .map(|p| (p.clone(), p.clone()))
                        .collect(),
                    "malloc" => env
                        .allocators
                        .iter()
                        .map(|a| (a.clone(), a.clone()))
                        .collect(),
                    "world" => params::WORLDS
                        .iter()
                        .map(|w| (w.to_string(), w.to_string()))
                        .collect(),
                    _ => Vec::new(),
                };
                if def.kind == Kind::Combo {
                    // Editable: a text field with the suggestions in a dropdown beside it.
                    let field =
                        Rect::from_min_max(rect.min, pos2(rect.right() - 20.0, rect.bottom()));
                    let mut text = current.clone();
                    let mut changed = text_box(ui, id, field, &mut text);
                    let arrow = Rect::from_min_max(pos2(field.right(), rect.top()), rect.max);
                    if let Some(picked) = combo(ui, gfx, id.with("list"), arrow, "", &options) {
                        text = picked;
                        changed = true;
                    }
                    if changed {
                        settings.set(setting, text);
                    }
                    changed
                } else if let Some(picked) = combo(ui, gfx, id, rect, &current, &options) {
                    settings.set(setting, picked);
                    true
                } else {
                    false
                }
            }
            Kind::TextValue => {
                let mut text = current;
                let changed = text_box(ui, id, rect, &mut text);
                if changed {
                    settings.set(setting, text);
                }
                changed
            }
            Kind::File | Kind::Folder | Kind::FolderList => {
                let field = Rect::from_min_max(rect.min, pos2(rect.right() - 22.0, rect.bottom()));
                let mut text = current;
                let changed = text_box(ui, id, field, &mut text);
                if changed {
                    settings.set(setting, text);
                }
                let button = Rect::from_min_max(pos2(field.right() + 1.0, rect.top()), rect.max);
                if browse_button(ui, id.with("browse"), button) {
                    let (tx, kind, filter, value) =
                        (self.picked_tx.clone(), def.kind, def.filter, def.value);
                    std::thread::spawn(move || {
                        let dialog = rfd::FileDialog::new();
                        let picked = if kind == Kind::File {
                            let patterns: Vec<String> = filter
                                .split(';')
                                .map(|p| p.trim().trim_start_matches("*.").to_owned())
                                .filter(|p| !p.is_empty() && p != "*")
                                .collect();
                            let dialog = if patterns.is_empty() {
                                dialog
                            } else {
                                dialog.add_filter("Files", &patterns)
                            };
                            dialog.pick_file()
                        } else {
                            dialog.pick_folder()
                        };
                        if let Some(path) = picked {
                            let _ = tx.send((value, path, kind == Kind::FolderList));
                        }
                    });
                }
                changed
            }
        }
    }
}
