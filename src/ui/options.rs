//! The Options page (`Views/Options/Options.xaml`), opened from the OPTIONS menu: launcher
//! configuration and the watched mod folders.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Align2, Id, Rect, Sense, pos2, vec2};

use super::gfx::Gfx;
use super::widgets::{combo, flat_button, flat_button_width, section_header, tab_control};
use super::{icons, theme};
use crate::params::Settings;
use crate::vanilla::local_mods::LocalMods;
use crate::vanilla::wine_path;

/// `LauncherShutdownMode` values and labels.
const SHUTDOWN_MODES: [(&str, &str); 4] = [
    ("DoNothing", "Do nothing"),
    ("Minimize", "Minimize Launcher"),
    ("CloseBefore", "Close Launcher after clicking Play"),
    ("CloseAfter", "Close Launcher after Arma 3 ends"),
];

/// `DefaultPageIndex` values and labels.
const DEFAULT_PAGES: [(&str, &str); 6] = [
    ("9", "Dashboard"),
    ("0", "News"),
    ("3", "DLC"),
    ("1", "Parameters"),
    ("2", "Mods"),
    ("8", "Servers"),
];

/// `ThemeIndex` values and labels.
const THEMES: [(&str, &str); 5] = [
    ("1", "Legacy Blue"),
    ("2", "Decent Black"),
    ("3", "Army Green"),
    ("5", "Apex"),
    ("6", "Laws of War"),
];

pub struct OptionsPage {
    tab: usize,
    folders_tx: Sender<PathBuf>,
    folders_rx: Receiver<PathBuf>,
}

impl OptionsPage {
    pub fn new() -> Self {
        let (folders_tx, folders_rx) = channel();
        Self {
            tab: 0,
            folders_tx,
            folders_rx,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        page: Rect,
        settings: &mut Settings,
        local: &mut LocalMods,
        launcher_data: &Path,
        game_dir: &Path,
    ) {
        while let Ok(folder) = self.folders_rx.try_recv() {
            add_watched(local, launcher_data, &folder);
        }
        ui.painter().rect_filled(page, 0.0, theme::PANEL);
        let (tab, content) = tab_control(
            ui,
            "options-tabs",
            page,
            &["Launcher options", "Mods options"],
            self.tab,
        );
        self.tab = tab;
        ui.painter().rect_filled(content, 0.0, theme::STRONG_PANEL);
        if self.tab == 0 {
            launcher_options(ui, gfx, content, settings);
        } else {
            self.mods_options(ui, gfx, content, local, launcher_data, game_dir);
        }
    }

    fn mods_options(
        &mut self,
        ui: &mut egui::Ui,
        gfx: &mut Gfx,
        content: Rect,
        local: &mut LocalMods,
        launcher_data: &Path,
        game_dir: &Path,
    ) {
        let header = Rect::from_min_size(content.min, vec2(content.width(), 50.0));
        section_header(
            ui,
            Id::unique("options-watched"),
            header,
            "Watched Folders",
            None,
            None,
        );
        let font = theme::body(theme::BODY_SIZE);
        ui.painter().text(
            pos2(content.left() + 20.0, header.bottom() + 10.0),
            Align2::LEFT_TOP,
            "Mods found in these folders are added to the mod list automatically.",
            font.clone(),
            theme::DIM_NAME,
        );
        // The folder list: 40 px rows with a folder icon, the path and a delete button.
        let list = Rect::from_min_size(
            pos2(content.left() + 20.0, header.bottom() + 36.0),
            vec2(content.width() - 40.0, 210.0),
        );
        ui.painter().rect_filled(list, 0.0, theme::BLACK);
        ui.painter().rect_stroke(
            list,
            0.0,
            (1.0, theme::argb(0xFF333333)),
            egui::StrokeKind::Inside,
        );
        let mut remove = None;
        for (i, folder) in local.autodetection_directories.iter().enumerate() {
            let row = Rect::from_min_size(
                pos2(list.left() + 1.0, list.top() + 1.0 + i as f32 * 40.0),
                vec2(list.width() - 2.0, 40.0),
            );
            if row.bottom() > list.bottom() {
                break;
            }
            let icon = Rect::from_min_size(
                pos2(row.left() + 10.0, row.center().y - 8.0),
                vec2(20.0, 16.0),
            );
            gfx.paint_icon(
                ui,
                icons::LOCAL_MOD,
                icon,
                vec2(20.0, 16.0),
                theme::argb(0xFFEAA32D),
            );
            ui.painter().text(
                pos2(icon.right() + 10.0, row.center().y),
                Align2::LEFT_CENTER,
                folder,
                font.clone(),
                theme::WHITE,
            );
            let delete = Rect::from_min_size(pos2(row.right() - 32.0, row.top()), vec2(32.0, 40.0));
            let response = ui.interact(delete, Id::unique(("watched-delete", i)), Sense::click());
            if response.hovered() {
                ui.painter()
                    .rect_filled(delete, 0.0, theme::argb(0x7FFFFFFF));
            }
            gfx.paint_icon(
                ui,
                icons::DELETE_ICON,
                delete,
                vec2(9.0, 12.0),
                theme::WHITE,
            );
            if response.clicked() {
                remove = Some(i);
            }
        }
        if let Some(i) = remove {
            local.autodetection_directories.remove(i);
            let _ = local.save(launcher_data);
        }
        let mut right = list.right();
        for (label, arma) in [
            ("Add 'Arma 3' folder", true),
            ("Add custom folder...", false),
        ] {
            let w = flat_button_width(ui, label).max(100.0);
            let button = Rect::from_min_size(pos2(right - w, list.bottom() + 10.0), vec2(w, 30.0));
            right -= w + 10.0;
            if flat_button(ui, Id::unique(("watched-add", arma)), button, label).clicked() {
                if arma {
                    add_watched(local, launcher_data, game_dir);
                } else {
                    let tx = self.folders_tx.clone();
                    std::thread::spawn(move || {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            let _ = tx.send(folder);
                        }
                    });
                }
            }
        }
    }
}

fn add_watched(local: &mut LocalMods, launcher_data: &Path, folder: &Path) {
    let windows = wine_path(folder);
    if !local
        .autodetection_directories
        .iter()
        .any(|d| d.eq_ignore_ascii_case(&windows))
    {
        local.autodetection_directories.push(windows);
        let _ = local.save(launcher_data);
    }
}

fn launcher_options(ui: &mut egui::Ui, gfx: &mut Gfx, content: Rect, settings: &mut Settings) {
    let header = Rect::from_min_size(content.min, vec2(content.width(), 50.0));
    section_header(
        ui,
        Id::unique("options-configuration"),
        header,
        "Configuration",
        None,
        None,
    );
    let font = theme::body(theme::BODY_SIZE);
    let rows: [OptionRow; 4] = [
        (
            "Language:",
            "CurrentLanguage",
            vec![("English".into(), "English".into())],
            200.0,
        ),
        (
            "Action after game start:",
            "LauncherShutdownMode",
            owned(&SHUTDOWN_MODES),
            300.0,
        ),
        (
            "Default page:",
            "DefaultPageIndex",
            owned(&DEFAULT_PAGES),
            200.0,
        ),
        ("Active theme:", "ThemeIndex", owned(&THEMES), 200.0),
    ];
    let label_width = rows
        .iter()
        .map(|(label, ..)| {
            ui.painter()
                .layout_no_wrap(label.to_string(), font.clone(), theme::WHITE)
                .size()
                .x
        })
        .fold(0.0, f32::max);
    let mut y = header.bottom() + 10.0;
    for (label, setting, options, width) in rows {
        let row = Rect::from_min_size(
            pos2(content.left() + 20.0, y),
            vec2(content.width() - 30.0, 36.0),
        );
        ui.painter().text(
            pos2(row.left(), row.center().y),
            Align2::LEFT_CENTER,
            label,
            font.clone(),
            theme::WHITE,
        );
        let field = Rect::from_min_size(
            pos2(row.left() + label_width + 10.0, row.top() + 5.0),
            vec2(width, 26.0),
        );
        let mut current = settings.get(setting);
        if setting == "ThemeIndex" && current == "0" {
            current = "2".into();
        }
        if setting == "CurrentLanguage" {
            current = "English".into();
        }
        if let Some(picked) = combo(
            ui,
            gfx,
            Id::unique(("option", setting)),
            field,
            &current,
            &options,
        ) && setting != "CurrentLanguage"
        {
            settings.set(setting, picked);
        }
        y += 36.0;
    }
}

/// An options row: label, setting, (value, label) choices, field width.
type OptionRow = (&'static str, &'static str, Vec<(String, String)>, f32);

fn owned(options: &[(&str, &str)]) -> Vec<(String, String)> {
    options
        .iter()
        .map(|(v, l)| (v.to_string(), l.to_string()))
        .collect()
}

/// The background image for `ThemeIndex` (`LauncherViewModel`); `None` for no image.
pub fn theme_background(index: &str) -> Option<&'static str> {
    match index {
        "0" | "2" => Some("bg_black.jpg"),
        "3" => Some("bg_army.jpg"),
        "4" => None,
        "5" => Some("bg_apex.jpg"),
        "6" => Some("bg_low.jpg"),
        _ => Some("bg_blue.jpg"),
    }
}
