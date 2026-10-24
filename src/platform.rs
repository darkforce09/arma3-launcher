//! Folders and memory that differ between the Proton prefix and a native Windows install.
//!
//! Linux keeps the XDG paths. Windows uses `%LOCALAPPDATA%\arma3-launcher` for this launcher's
//! own files, and the known Documents folder (OneDrive redirection included) for Arma profiles.

use std::io::Write;
use std::path::PathBuf;

/// This launcher's state directory (`state.json`, the log).
pub fn state_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        Some(local_app_data()?.join("arma3-launcher"))
    }
    #[cfg(not(windows))]
    {
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
        Some(base.join("arma3-launcher"))
    }
}

/// Downloaded images (`…/http`).
pub fn cache_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        Some(local_app_data()?.join("arma3-launcher").join("http"))
    }
    #[cfg(not(windows))]
    {
        let base = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
        Some(base.join("arma3-launcher").join("http"))
    }
}

/// `%LOCALAPPDATA%` on Windows. Linux callers use the Proton prefix instead.
#[cfg(windows)]
pub fn local_app_data() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}

/// The user's Documents folder. On Windows this follows `FOLDERID_Documents`.
#[cfg(windows)]
pub fn documents_dir() -> Option<PathBuf> {
    known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Documents)
        .or_else(|| std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join("Documents")))
}

/// Total installed memory in MB, the `maxmem` maximum. `None` when it can't be read.
pub fn total_memory_mb() -> Option<i64> {
    #[cfg(windows)]
    {
        installed_memory_mb()
    }
    #[cfg(not(windows))]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kb = text.lines().find_map(|line| {
            let rest = line.strip_prefix("MemTotal:")?;
            rest.split_whitespace().next()?.parse::<i64>().ok()
        })?;
        Some(kb / 1024)
    }
}

/// Appends `message` to stderr and to `launcher.log` in the state directory. The Windows GUI
/// subsystem drops stderr, so the file is the record Steam's console won't show.
pub fn log_error(message: &str) {
    eprintln!("arma3-launcher: {message}");
    let Some(dir) = state_dir() else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("launcher.log"))
    {
        Ok(file) => file,
        Err(_) => return,
    };
    let _ = writeln!(file, "{message}");
}

/// Turns off Windows 11's rounded corners, which clip the painted shadow. Retries until the
/// window exists.
#[cfg(windows)]
pub fn disable_rounded_corners() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static DONE: AtomicBool = AtomicBool::new(false);
    if DONE.load(Ordering::Relaxed) {
        return;
    }
    let title: Vec<u16> = "Arma 3 Launcher"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let hwnd = unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW(std::ptr::null(), title.as_ptr())
    };
    if hwnd.is_null() {
        return;
    }
    // DWMWCP_DONOTROUND.
    let preference: u32 = 1;
    let hr = unsafe {
        windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute(
            hwnd,
            windows_sys::Win32::Graphics::Dwm::DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &preference as *const u32 as *const _,
            std::mem::size_of_val(&preference) as u32,
        )
    };
    if hr == 0 {
        DONE.store(true, Ordering::Relaxed);
    }
}

/// An Arma 3 install found from Steam's library folders, when the launcher was not started
/// through `%command%`.
pub fn discover_game_dir() -> Option<PathBuf> {
    for root in steam_roots() {
        for folders in [
            root.join("steamapps/libraryfolders.vdf"),
            root.join("config/libraryfolders.vdf"),
        ] {
            let Ok(text) = std::fs::read_to_string(&folders) else {
                continue;
            };
            let Some(library) = crate::vdf::library_for_app(&text, "107410") else {
                continue;
            };
            let game = PathBuf::from(library).join("steamapps/common/Arma 3");
            if game.is_dir() {
                return Some(game);
            }
        }
        let direct = root.join("steamapps/common/Arma 3");
        if direct.is_dir() {
            return Some(direct);
        }
    }
    None
}

fn steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    {
        if let Some(path) = reg_sz(
            windows_sys::Win32::System::Registry::HKEY_CURRENT_USER,
            r"Software\Valve\Steam",
            "SteamPath",
        ) {
            roots.push(PathBuf::from(path));
        }
        if let Some(path) = reg_sz(
            windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Valve\Steam",
            "InstallPath",
        ) {
            roots.push(PathBuf::from(path));
        }
        roots.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    }
    #[cfg(not(windows))]
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        roots.push(home.join(".local/share/Steam"));
        roots.push(home.join(".steam/steam"));
        roots.push(home.join(".steam/root"));
    }
    roots
}

#[cfg(windows)]
fn known_folder(id: &windows_sys::core::GUID) -> Option<PathBuf> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::SHGetKnownFolderPath;
    unsafe {
        let mut raw = std::ptr::null_mut();
        let hr = SHGetKnownFolderPath(id, Default::default(), std::ptr::null_mut(), &mut raw);
        if hr != 0 || raw.is_null() {
            return None;
        }
        let mut len = 0usize;
        while *raw.add(len) != 0 {
            len += 1;
        }
        let path = String::from_utf16_lossy(std::slice::from_raw_parts(raw, len));
        CoTaskMemFree(raw.cast());
        Some(PathBuf::from(path))
    }
}

#[cfg(windows)]
fn installed_memory_mb() -> Option<i64> {
    use windows_sys::Win32::System::SystemInformation::{
        GetPhysicallyInstalledSystemMemory, GlobalMemoryStatusEx, MEMORYSTATUSEX,
    };
    unsafe {
        let mut kilobytes = 0u64;
        if GetPhysicallyInstalledSystemMemory(&mut kilobytes) != 0 && kilobytes > 0 {
            return Some((kilobytes / 1024) as i64);
        }
        let mut status = std::mem::zeroed::<MEMORYSTATUSEX>();
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut status) == 0 {
            return None;
        }
        Some((status.ullTotalPhys / (1024 * 1024)) as i64)
    }
}

#[cfg(windows)]
fn reg_sz(
    root: windows_sys::Win32::System::Registry::HKEY,
    subkey: &str,
    value: &str,
) -> Option<String> {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{RRF_RT_REG_SZ, RegGetValueW};
    let subkey = wide(subkey);
    let value = wide(value);
    let mut buf = vec![0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    let err = unsafe {
        RegGetValueW(
            root,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut len,
        )
    };
    if err != ERROR_SUCCESS || len < 2 {
        return None;
    }
    let chars = (len as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..chars]))
}

#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The Steamworks DLL this binary was linked against. The Arma 3 folder has an older
/// `steam_api64.dll` that does not export the flat API (`SteamAPI_SteamFriends_v018`), so a
/// launcher placed there must not load that file.
#[cfg(windows)]
const STEAM_API_DLL: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/steam_api64.dll"));

/// `DelayLoadInfo` from `delayimp.h`, x64. Only `sz_dll` is read.
#[cfg(windows)]
#[repr(C)]
struct DelayLoadInfo {
    cb: u32,
    _pad: u32,
    descr: *const u8,
    function_slot: *mut *mut u8,
    sz_dll: *const i8,
}

#[cfg(windows)]
const _: () = assert!(std::mem::offset_of!(DelayLoadInfo, sz_dll) == 24);

/// Delay-load notify hook (`dliNotePreLoadLibrary`). `__pfnDliNotifyHook2` is the name
/// `delayimp.lib` calls; defining it replaces the null hook in that library.
#[cfg(windows)]
#[used]
#[allow(non_upper_case_globals)]
#[unsafe(no_mangle)]
static __pfnDliNotifyHook2: unsafe extern "system" fn(
    u32,
    *mut DelayLoadInfo,
) -> *mut std::ffi::c_void = delay_hook;

/// Referenced from `main` so the delay-load hook is part of the process before Steam is used.
#[cfg(windows)]
pub fn prepare_steam_api() {
    let _ = __pfnDliNotifyHook2 as usize;
}

#[cfg(windows)]
unsafe extern "system" fn delay_hook(
    notify: u32,
    info: *mut DelayLoadInfo,
) -> *mut std::ffi::c_void {
    // `dliNotePreLoadLibrary`.
    if notify != 1 || info.is_null() {
        return std::ptr::null_mut();
    }
    let dll_name = unsafe { (*info).sz_dll };
    if dll_name.is_null() {
        return std::ptr::null_mut();
    }
    let name = unsafe { std::ffi::CStr::from_ptr(dll_name) };
    if !name.to_bytes().eq_ignore_ascii_case(b"steam_api64.dll") {
        return std::ptr::null_mut();
    }
    let Some(path) = ensure_steam_dll() else {
        log_error("could not write the private Steam library");
        return std::ptr::null_mut();
    };
    let wide_path = wide_os(path.as_os_str());
    let module =
        unsafe { windows_sys::Win32::System::LibraryLoader::LoadLibraryW(wide_path.as_ptr()) };
    if module.is_null() {
        log_error(&format!(
            "loading the private Steam library failed ({})",
            std::io::Error::last_os_error()
        ));
    }
    module
}

/// Writes the embedded DLL under `%LOCALAPPDATA%\arma3-launcher`, away from the game's copy.
#[cfg(windows)]
fn ensure_steam_dll() -> Option<PathBuf> {
    let dir = local_app_data()?.join("arma3-launcher");
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("a3l_steam_api.dll");
    if std::fs::read(&path).ok().as_deref() != Some(STEAM_API_DLL) {
        std::fs::write(&path, STEAM_API_DLL).ok()?;
    }
    Some(path)
}

#[cfg(windows)]
fn wide_os(text: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    text.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(all(test, windows))]
mod tests {
    use super::STEAM_API_DLL;

    #[test]
    fn embedded_steam_api_exports_the_flat_interface() {
        let needle = b"SteamAPI_SteamFriends_v018";
        assert!(
            STEAM_API_DLL
                .windows(needle.len())
                .any(|window| window == needle)
        );
    }
}
