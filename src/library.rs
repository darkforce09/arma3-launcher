//! The loaded mods and presets: what the Mods page edits and what PLAY sends to the game.

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;

use crate::mods::{self, Mod, Source, Status};
use crate::steam::{ItemDetails, ItemSnapshot};
use crate::vanilla::game_path;
use crate::vanilla::preset::{self, DEFAULT_PRESET_FILE, Preset};

/// A dependency in a mod's chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    /// `steam:<id>`.
    pub id: String,
    pub workshop_id: u64,
    pub name: String,
    /// In the library: subscribed, installed or on its way.
    pub subscribed: bool,
    pub loaded: bool,
}

/// What a merge of Steam's items changed: workshop ids new to the library, and mods installed for
/// the first time (not updates).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Merged {
    pub added: Vec<u64>,
    pub installed: Vec<u64>,
}

pub struct Library {
    /// All mods, sorted by name.
    pub mods: Vec<Mod>,
    /// Identifiers of the loaded mods, in load order.
    pub loaded: Vec<String>,
    /// Loaded DLC app ids, in load order.
    pub dlcs: Vec<u32>,
    pub presets: Vec<(String, Preset)>,
    /// The preset last applied or saved; `None` for the unnamed one.
    pub active_preset: Option<String>,
    /// The workshop item Steam is downloading: name, bytes done, bytes total.
    pub download: Option<(String, u64, u64)>,
    presets_dir: PathBuf,
}

impl Library {
    /// The library with the loaded state of the last session (`arma3.defaultpreset2`).
    pub fn new(mods: Vec<Mod>, presets_dir: PathBuf, active_preset: Option<String>) -> Self {
        let default = Preset::load(&presets_dir.join(DEFAULT_PRESET_FILE)).unwrap_or_default();
        let presets = preset::named_presets(&presets_dir);
        let active_preset = active_preset.filter(|name| presets.iter().any(|(n, _)| n == name));
        let mut library = Self {
            mods,
            loaded: Vec::new(),
            dlcs: default.dlcs,
            presets,
            active_preset,
            download: None,
            presets_dir,
        };
        library.loaded = library.known(&default.mods);
        library
    }

    pub fn find(&self, id: &str) -> Option<&Mod> {
        self.mods.iter().find(|m| m.id.eq_ignore_ascii_case(id))
    }

    fn index(&self) -> HashMap<&str, &Mod> {
        self.mods.iter().map(|m| (m.id.as_str(), m)).collect()
    }

    /// `ids` that name a mod in the library, normalised to the library's spelling.
    fn known(&self, ids: &[String]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for id in ids {
            if let Some(m) = self.find(id)
                && !out.contains(&m.id)
            {
                out.push(m.id.clone());
            }
        }
        out
    }

    pub fn is_loaded(&self, id: &str) -> bool {
        self.loaded.iter().any(|l| l == id)
    }

    pub fn unload(&mut self, id: &str) {
        self.loaded.retain(|l| l != id);
    }

    /// The dependency badge of a mod: its dependency count, and whether all are satisfied (loaded
    /// when the mod is, otherwise in the library). `None` without dependencies.
    pub fn dependency_badge(&self, m: &Mod) -> Option<(usize, bool)> {
        if m.dependencies.is_empty() {
            return None;
        }
        let loaded = self.is_loaded(&m.id);
        let satisfied = m.dependencies.iter().all(|d| {
            if loaded {
                self.is_loaded(d)
            } else {
                self.find(d).is_some()
            }
        });
        Some((m.dependencies.len(), satisfied))
    }

    /// The status shown for a mod (`UpdateModState`): a healthy mod whose dependencies aren't
    /// satisfied, by the badge's rule, shows Dependencies missing.
    pub fn status(&self, m: &Mod) -> Status {
        match self.dependency_badge(m) {
            Some((_, false)) if m.status == Status::Ok => Status::DependenciesMissing,
            _ => m.status,
        }
    }

    /// Loads exactly `ids` at `index` (the end when `None`), without pulling in dependencies;
    /// the load order then puts dependencies first. Ids not in the library yet (subscribed,
    /// still downloading) are kept and show up once installed.
    pub fn load_exactly(&mut self, ids: &[String], index: Option<usize>) {
        let add: Vec<String> = ids
            .iter()
            .filter(|id| !self.is_loaded(id))
            .map(|id| self.find(id).map_or_else(|| id.clone(), |m| m.id.clone()))
            .collect();
        let at = index.unwrap_or(self.loaded.len()).min(self.loaded.len());
        self.loaded.splice(at..at, add);
        let mut seen = std::collections::HashSet::new();
        self.loaded.retain(|id| seen.insert(id.clone()));
        self.loaded = mods::dependency_order(&self.loaded, &self.index());
    }

    /// The dependencies of `originators` (`PreviewDependencyChain`), 3 levels deep like vanilla:
    /// a library mod's own dependency list, else the item's Steam details. Missing items appear
    /// once Steam has sent their details (only those tagged "Mod"); the second list names the
    /// items whose details are still unknown.
    pub fn dependency_chain(
        &self,
        originators: &[String],
        details: &HashMap<u64, ItemDetails>,
    ) -> (Vec<Dependency>, Vec<u64>) {
        let mut seen: std::collections::HashSet<String> =
            originators.iter().map(|o| o.to_ascii_lowercase()).collect();
        let mut level: Vec<String> = originators.to_vec();
        let (mut chain, mut unknown) = (Vec::new(), Vec::new());
        for _ in 0..3 {
            let mut next = Vec::new();
            for id in &level {
                let children: Vec<String> = match self.find(id) {
                    Some(m) => m.dependencies.clone(),
                    None => mods::workshop_number(id)
                        .and_then(|n| details.get(&n))
                        .map(|d| d.children.iter().map(|c| mods::steam_id(*c)).collect())
                        .unwrap_or_default(),
                };
                for child in children {
                    if !seen.insert(child.to_ascii_lowercase()) {
                        continue;
                    }
                    let Some(number) = mods::workshop_number(&child) else {
                        continue;
                    };
                    let (name, subscribed) = match (self.find(&child), details.get(&number)) {
                        (Some(m), _) => (m.name.clone(), true),
                        (None, Some(d)) if d.is_mod => (d.title.clone(), false),
                        (None, Some(_)) => continue,
                        (None, None) => {
                            unknown.push(number);
                            continue;
                        }
                    };
                    chain.push(Dependency {
                        loaded: self.is_loaded(&child),
                        id: child.clone(),
                        workshop_id: number,
                        name,
                        subscribed,
                    });
                    next.push(child);
                }
            }
            level = next;
        }
        chain.sort_by_key(|d| d.name.to_lowercase());
        (chain, unknown)
    }

    /// The loaded dependencies, at any depth, of mods about to be unloaded
    /// (`FindOrphandedDependencies`), leaving out those a mod that stays loaded still needs.
    pub fn orphaned_dependencies(&self, unloading: &[String]) -> Vec<String> {
        let candidates: Vec<String> = self
            .dependencies_of(unloading)
            .into_iter()
            .filter(|d| self.is_loaded(d))
            .collect();
        let staying: Vec<String> = self
            .loaded
            .iter()
            .filter(|l| !unloading.contains(l) && !candidates.contains(l))
            .cloned()
            .collect();
        let needed: std::collections::HashSet<String> =
            self.dependencies_of(&staying).into_iter().collect();
        candidates
            .into_iter()
            .filter(|c| !needed.contains(c))
            .collect()
    }

    /// The dependencies of `roots` at any depth, through the library's mods.
    fn dependencies_of(&self, roots: &[String]) -> Vec<String> {
        let mut seen: std::collections::HashSet<String> = roots.iter().cloned().collect();
        let mut stack: Vec<String> = roots.to_vec();
        let mut out = Vec::new();
        while let Some(id) = stack.pop() {
            let Some(m) = self.find(&id) else { continue };
            for dep in &m.dependencies {
                if seen.insert(dep.clone()) {
                    out.push(dep.clone());
                    stack.push(dep.clone());
                }
            }
        }
        out
    }

    pub fn unload_all(&mut self, visible: impl Fn(&Mod) -> bool) {
        let keep: Vec<String> = self
            .loaded
            .iter()
            .filter(|id| self.find(id).is_some_and(|m| !visible(m)))
            .cloned()
            .collect();
        self.loaded = keep;
    }

    /// Moves the loaded mod at `from` to position `to` (in the list without it).
    pub fn move_loaded(&mut self, from: usize, to: usize) {
        if from < self.loaded.len() {
            let id = self.loaded.remove(from);
            self.loaded.insert(to.min(self.loaded.len()), id);
        }
    }

    pub fn preset(&self, name: &str) -> Option<&Preset> {
        self.presets.iter().find(|(n, _)| n == name).map(|(_, p)| p)
    }

    /// The active preset's name with `*` when the loaded mods differ from it, or vanilla's
    /// "Unsaved preset".
    pub fn preset_label(&self) -> (String, bool) {
        match self
            .active_preset
            .as_deref()
            .and_then(|n| self.preset(n).map(|p| (n, p)))
        {
            Some((name, preset)) => (
                name.to_owned(),
                self.known(&preset.mods) != self.loaded || preset.dlcs != self.dlcs,
            ),
            None => ("Unsaved preset".to_owned(), false),
        }
    }

    /// Loads exactly the preset's mods and DLC (`SetExactly`).
    pub fn apply_preset(&mut self, name: &str) {
        let Some(preset) = self.preset(name).cloned() else {
            return;
        };
        self.loaded = self.known(&preset.mods);
        self.dlcs = preset.dlcs;
        self.active_preset = Some(name.to_owned());
    }

    /// Writes preset `name` from `mods` without changing what is loaded.
    pub fn write_preset(&mut self, name: &str, mods: Vec<String>) -> io::Result<()> {
        let preset = Preset::new(mods, Vec::new());
        preset.save(&preset::preset_path(&self.presets_dir, name))?;
        match self.presets.iter_mut().find(|(n, _)| n == name) {
            Some((_, stored)) => *stored = preset,
            None => {
                self.presets.push((name.to_owned(), preset));
                self.presets.sort_by_key(|(n, _)| n.to_lowercase());
            }
        }
        Ok(())
    }

    /// Saves the loaded mods and DLC as preset `name` and makes it active.
    pub fn save_preset(&mut self, name: &str) -> io::Result<()> {
        let preset = Preset::new(self.loaded.clone(), self.dlcs.clone());
        preset.save(&preset::preset_path(&self.presets_dir, name))?;
        match self.presets.iter_mut().find(|(n, _)| n == name) {
            Some((_, p)) => *p = preset,
            None => {
                self.presets.push((name.to_owned(), preset));
                self.presets.sort_by_key(|(n, _)| n.to_lowercase());
            }
        }
        self.active_preset = Some(name.to_owned());
        Ok(())
    }

    pub fn delete_preset(&mut self, name: &str) -> io::Result<()> {
        std::fs::remove_file(preset::preset_path(&self.presets_dir, name))?;
        self.presets.retain(|(n, _)| n != name);
        if self.active_preset.as_deref() == Some(name) {
            self.active_preset = None;
        }
        Ok(())
    }

    pub fn rename_preset(&mut self, from: &str, to: &str) -> io::Result<()> {
        std::fs::rename(
            preset::preset_path(&self.presets_dir, from),
            preset::preset_path(&self.presets_dir, to),
        )?;
        for (name, _) in &mut self.presets {
            if name == from {
                *name = to.to_owned();
            }
        }
        self.presets.sort_by_key(|(n, _)| n.to_lowercase());
        if self.active_preset.as_deref() == Some(from) {
            self.active_preset = Some(to.to_owned());
        }
        Ok(())
    }

    /// Writes the loaded state for the next session, as the vanilla launcher does at exit.
    pub fn save_default_preset(&self) -> io::Result<()> {
        Preset::new(self.loaded.clone(), self.dlcs.clone())
            .save(&self.presets_dir.join(DEFAULT_PRESET_FILE))
    }

    /// The value of `LastActivePreset` in `user.config`: the name, or empty for none.
    pub fn last_active_preset_setting(&self) -> String {
        self.active_preset.clone().unwrap_or_default()
    }

    /// The `-mod` entries for preset `name`, as [`Self::launch_mod_paths`] gives them for the loaded mods.
    pub fn preset_mod_paths(&self, name: &str) -> Vec<String> {
        let ids = self
            .preset(name)
            .map(|p| self.known(&p.mods))
            .unwrap_or_default();
        self.mod_paths(&ids)
    }

    /// The `-mod` entries for the loaded mods that can run: dependencies first, as the game sees them.
    pub fn launch_mod_paths(&self) -> Vec<String> {
        self.mod_paths(&self.loaded)
    }

    fn mod_paths(&self, ids: &[String]) -> Vec<String> {
        let index = self.index();
        mods::dependency_order(ids, &index)
            .iter()
            .filter_map(|id| index.get(id.as_str()))
            .filter(|m| m.launchable())
            .map(|m| game_path(&m.path))
            .collect()
    }

    /// Merges what Steam reports about subscribed items: install folders, downloads, and items
    /// subscribed but not yet on disk.
    pub fn merge_steam_items(
        &mut self,
        items: &[ItemSnapshot],
        details: &HashMap<u64, ItemDetails>,
    ) -> Merged {
        let mut merged = Merged::default();
        for item in items {
            let steam_status = if item.downloading {
                Status::Busy
            } else if item.download_pending || !item.installed {
                Status::InstallationQueued
            } else if item.needs_update {
                Status::UpdateAvailable
            } else {
                Status::Ok
            };
            // Installed per Steam: Ok when the folder holds PBOs, else Corrupted.
            let checked = |path: &std::path::Path| {
                if mods::is_intact(path) {
                    Status::Ok
                } else {
                    Status::Corrupted
                }
            };
            let id = mods::steam_id(item.id);
            match self.mods.iter_mut().find(|m| m.id == id) {
                Some(m) => {
                    let moved = item.folder.as_ref().is_some_and(|f| *f != m.path);
                    let was_on_disk = !m.path.as_os_str().is_empty();
                    if let Some(folder) = &item.folder {
                        m.path = folder.clone();
                    }
                    let intact = m.status == Status::Ok;
                    m.status = match steam_status {
                        // Already checked at this path: no disk access on every poll.
                        Status::Ok if intact && !moved => m.status,
                        Status::Ok => checked(&m.path),
                        other => other,
                    };
                    if !was_on_disk && m.status == Status::Ok {
                        merged.installed.push(item.id);
                    }
                }
                None => {
                    let name = details
                        .get(&item.id)
                        .map(|d| d.title.clone())
                        .filter(|t| !t.is_empty())
                        .unwrap_or_else(|| item.id.to_string());
                    let path = item.folder.clone().unwrap_or_default();
                    let status = match steam_status {
                        Status::Ok => checked(&path),
                        other => other,
                    };
                    self.mods.push(Mod {
                        id,
                        source: Source::Workshop(item.id),
                        name,
                        author: String::new(),
                        path,
                        dependencies: Vec::new(),
                        status,
                        updated: false,
                        auto_included: false,
                        last_update: None,
                    });
                    if status == Status::Ok {
                        merged.installed.push(item.id);
                    }
                    merged.added.push(item.id);
                }
            }
        }
        self.download = items.iter().find_map(|item| {
            let (done, total) = item.progress.filter(|(_, total)| *total > 0)?;
            let name = self
                .mods
                .iter()
                .find(|m| m.workshop_id() == Some(item.id))
                .map(|m| m.name.clone())?;
            Some((name, done, total))
        });
        self.mods.sort_by_key(|m| m.name.to_lowercase());
        merged
    }

    /// Sets a workshop mod's author from Steam unless one is known already.
    pub fn set_author(&mut self, workshop_id: u64, name: &str) {
        if let Some(m) = self
            .mods
            .iter_mut()
            .find(|m| m.workshop_id() == Some(workshop_id) && m.author.is_empty())
        {
            m.author = name.to_owned();
        }
    }

    /// Applies workshop details (titles, dependencies, update times).
    pub fn merge_details(&mut self, details: &[ItemDetails]) {
        for d in details {
            if let Some(m) = self.mods.iter_mut().find(|m| m.workshop_id() == Some(d.id)) {
                if !d.title.is_empty() {
                    m.name = d.title.clone();
                }
                m.dependencies = d.children.iter().map(|c| mods::steam_id(*c)).collect();
                if d.time_updated > 0 {
                    m.last_update = Some(d.time_updated);
                }
            }
        }
        self.mods.sort_by_key(|m| m.name.to_lowercase());
    }

    /// Removes an unsubscribed workshop item.
    pub fn remove(&mut self, id: &str) {
        self.unload(id);
        self.mods.retain(|m| m.id != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library(ids: &[(&str, &[&str])]) -> Library {
        let mods = ids
            .iter()
            .map(|(id, deps)| Mod {
                id: id.to_string(),
                source: Source::Workshop(0),
                name: id.to_string(),
                author: String::new(),
                path: PathBuf::from(format!("/mods/{id}")),
                dependencies: deps.iter().map(|d| d.to_string()).collect(),
                status: Status::Ok,
                updated: false,
                auto_included: false,
                last_update: None,
            })
            .collect();
        Library {
            mods,
            loaded: vec![],
            dlcs: vec![],
            presets: vec![],
            active_preset: None,
            download: None,
            presets_dir: PathBuf::new(),
        }
    }

    #[test]
    fn loaded_mods_launch_with_dependencies_first() {
        let mut lib = library(&[
            ("steam:1", &["steam:2"]),
            ("steam:2", &[]),
            ("steam:3", &[]),
        ]);
        lib.load_exactly(&["steam:3".into()], None);
        lib.load_exactly(&["steam:1".into(), "steam:2".into()], Some(0));
        assert_eq!(lib.loaded, ["steam:2", "steam:1", "steam:3"]);
        assert_eq!(
            lib.launch_mod_paths(),
            [
                game_path(&PathBuf::from("/mods/steam:2")),
                game_path(&PathBuf::from("/mods/steam:1")),
                game_path(&PathBuf::from("/mods/steam:3")),
            ]
        );
        lib.unload("steam:2");
        assert_eq!(lib.loaded, ["steam:1", "steam:3"]);
    }

    #[test]
    fn presets_track_modification() {
        let mut lib = library(&[("steam:1", &[]), ("steam:2", &[])]);
        lib.presets.push((
            "A".into(),
            Preset {
                last_update: "x".into(),
                mods: vec!["STEAM:2".into(), "steam:9".into()],
                dlcs: vec![],
            },
        ));
        lib.apply_preset("A");
        assert_eq!(lib.loaded, ["steam:2"]);
        assert_eq!(lib.preset_label(), ("A".into(), false));
        lib.load_exactly(&["steam:1".into()], None);
        assert_eq!(lib.preset_label(), ("A".into(), true));
    }

    #[test]
    fn a_subscribed_item_is_ok_once_steam_installs_it() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp/merge");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("1/addons")).unwrap();
        std::fs::write(root.join("1/addons/x.pbo"), b"").unwrap();
        std::fs::create_dir_all(root.join("2/addons")).unwrap();
        let item = |id: u64, folder: Option<PathBuf>, installed: bool| ItemSnapshot {
            id,
            folder,
            installed,
            needs_update: false,
            downloading: false,
            download_pending: false,
            progress: None,
        };
        let mut lib = library(&[]);
        let names = HashMap::new();
        // Subscribed, not on disk yet.
        assert_eq!(
            lib.merge_steam_items(&[item(1, None, false)], &names),
            Merged {
                added: vec![1],
                installed: vec![]
            }
        );
        assert_eq!(lib.mods[0].status, Status::InstallationQueued);
        // Installed: checked at the folder Steam reports, and reported as a first install.
        let installed = [item(1, Some(root.join("1")), true)];
        assert_eq!(
            lib.merge_steam_items(&installed, &names),
            Merged {
                added: vec![],
                installed: vec![1]
            }
        );
        assert_eq!(
            (lib.mods[0].status, &lib.mods[0].path),
            (Status::Ok, &root.join("1"))
        );
        // An update of an installed mod is no first install.
        let mut update = item(1, Some(root.join("1")), true);
        update.needs_update = true;
        lib.merge_steam_items(&[update], &names);
        assert_eq!(lib.mods[0].status, Status::UpdateAvailable);
        assert_eq!(lib.merge_steam_items(&installed, &names), Merged::default());
        // Steam's author name fills only an empty one.
        lib.set_author(1, "Ketzl");
        lib.set_author(1, "Someone else");
        assert_eq!(lib.mods[0].author, "Ketzl");
        // Without PBOs it is corrupted, and recovers once they are there.
        lib.merge_steam_items(&[item(2, Some(root.join("2")), true)], &names);
        let two = |lib: &Library| lib.find("steam:2").unwrap().status;
        assert_eq!(two(&lib), Status::Corrupted);
        std::fs::write(root.join("2/addons/y.pbo"), b"").unwrap();
        lib.merge_steam_items(&[item(2, Some(root.join("2")), true)], &names);
        assert_eq!(two(&lib), Status::Ok);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn details(id: u64, title: &str, children: &[u64], is_mod: bool) -> (u64, ItemDetails) {
        (
            id,
            ItemDetails {
                id,
                title: title.into(),
                owner: 0,
                time_updated: 0,
                children: children.to_vec(),
                size: 0,
                is_mod,
            },
        )
    }

    #[test]
    fn dependency_chains_follow_the_library_then_steam_three_levels_deep() {
        // 1 → 2 (installed, loaded) → 3 (missing, known) → 4 (missing, known) → 5 (too deep);
        // 1 → 6 (missing, not a mod); 3 → 7 (missing, details unknown).
        let mut lib = library(&[
            ("steam:1", &["steam:2", "steam:6"]),
            ("steam:2", &["steam:3"]),
        ]);
        lib.loaded = vec!["steam:2".into()];
        let steam: HashMap<u64, ItemDetails> = [
            details(3, "C", &[4, 7], true),
            details(4, "D", &[5], true),
            details(5, "E", &[], true),
            details(6, "Scenario", &[], false),
        ]
        .into();
        let (chain, unknown) = lib.dependency_chain(&["steam:1".into()], &steam);
        let found: Vec<(&str, bool, bool)> = chain
            .iter()
            .map(|d| (d.id.as_str(), d.subscribed, d.loaded))
            .collect();
        assert_eq!(
            found,
            [
                ("steam:3", false, false),
                ("steam:4", false, false),
                ("steam:2", true, true)
            ]
        );
        assert_eq!(unknown, [7]);
    }

    #[test]
    fn loading_exactly_adds_no_dependencies_and_orders_them_first() {
        let mut lib = library(&[("a", &["b"]), ("b", &[]), ("c", &["b"])]);
        lib.load_exactly(&["a".into()], None);
        assert_eq!(lib.loaded, ["a"]);
        lib.load_exactly(&["b".into(), "steam:9".into()], Some(0));
        assert_eq!(lib.loaded, ["b", "steam:9", "a"]);
        assert_eq!(lib.orphaned_dependencies(&["a".into()]), ["b"]);
        assert!(lib.orphaned_dependencies(&["b".into()]).is_empty());
    }

    #[test]
    fn orphaned_dependencies_leave_out_those_a_staying_mod_needs() {
        // a → c, b → c; a → x → d, m → x.
        let mut lib = library(&[
            ("a", &["c", "x"]),
            ("b", &["c"]),
            ("c", &[]),
            ("x", &["d"]),
            ("d", &[]),
            ("m", &["x"]),
        ]);
        lib.loaded = ["a", "b", "c", "x", "d"].map(String::from).to_vec();
        assert_eq!(lib.orphaned_dependencies(&["a".into()]), ["x", "d"]);
        let mut both = lib.orphaned_dependencies(&["a".into(), "b".into()]);
        both.sort();
        assert_eq!(both, ["c", "d", "x"]);
        lib.loaded.push("m".into());
        assert!(lib.orphaned_dependencies(&["a".into()]).is_empty());
        assert!(lib.orphaned_dependencies(&["b".into()]).is_empty());
    }

    #[test]
    fn missing_dependencies_follow_the_load_state() {
        let mut lib = library(&[("a", &[]), ("b", &["a"])]);
        lib.loaded = vec!["a".into(), "b".into()];
        assert_eq!(lib.status(&lib.mods[1]), Status::Ok);
        lib.unload("a");
        assert_eq!(lib.status(&lib.mods[1]), Status::DependenciesMissing);
        lib.unload("b");
        assert_eq!(lib.status(&lib.mods[1]), Status::Ok);
    }
}
