//! Textures for the launcher's bitmaps (decoded off the UI thread) and vector icons (rasterised
//! with resvg at the exact pixel size they are drawn at).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{
    self, Color32, ColorImage, Context, Pos2, Rect, TextureHandle, TextureOptions, Vec2,
};

use crate::assets::Assets;

/// Where the launcher's images live inside its resources.
pub const IMAGES: &str = "themes/defaultstyle/images/";

pub struct Gfx {
    assets: Option<Arc<Assets>>,
    bitmaps: HashMap<String, Bitmap>,
    icons: HashMap<(usize, u32, u32), TextureHandle>,
    decoded_tx: Sender<(String, Option<ColorImage>)>,
    decoded_rx: Receiver<(String, Option<ColorImage>)>,
}

enum Bitmap {
    Loading,
    Ready(TextureHandle),
    Missing,
}

impl Gfx {
    pub fn new(assets: Option<Arc<Assets>>) -> Self {
        let (decoded_tx, decoded_rx) = channel();
        Self {
            assets,
            bitmaps: HashMap::new(),
            icons: HashMap::new(),
            decoded_tx,
            decoded_rx,
        }
    }

    /// Starts decoding `names` in the background so they are ready by the time they are drawn.
    pub fn preload(&mut self, ctx: &Context, names: &[&str]) {
        for name in names {
            self.request(ctx, name);
        }
    }

    /// The texture of the image resource `name` (relative to [`IMAGES`]), once decoded.
    pub fn bitmap(&mut self, ctx: &Context, name: &str) -> Option<&TextureHandle> {
        self.receive(ctx);
        self.request(ctx, name);
        match self.bitmaps.get(name) {
            Some(Bitmap::Ready(texture)) => Some(texture),
            _ => None,
        }
    }

    fn request(&mut self, ctx: &Context, name: &str) {
        if self.bitmaps.contains_key(name) {
            return;
        }
        let Some(assets) = self.assets.clone() else {
            self.bitmaps.insert(name.to_owned(), Bitmap::Missing);
            return;
        };
        self.bitmaps.insert(name.to_owned(), Bitmap::Loading);
        let (tx, ctx, name) = (self.decoded_tx.clone(), ctx.clone(), name.to_owned());
        std::thread::spawn(move || {
            let image = assets
                .get(&format!("{IMAGES}{name}"))
                .and_then(|bytes| decode(bytes).ok());
            let _ = tx.send((name, image));
            ctx.request_repaint();
        });
    }

    /// An image from the web, cached on disk; `None` until it has loaded (or when it fails).
    pub fn remote(&mut self, ctx: &Context, url: &str) -> Option<&TextureHandle> {
        self.receive(ctx);
        let key = format!("url:{url}");
        if !self.bitmaps.contains_key(&key) {
            self.bitmaps.insert(key.clone(), Bitmap::Loading);
            let (tx, ctx, url, key) = (
                self.decoded_tx.clone(),
                ctx.clone(),
                url.to_owned(),
                key.clone(),
            );
            std::thread::spawn(move || {
                let image = fetch_cached(&url).and_then(|bytes| decode(&bytes).ok());
                let _ = tx.send((key, image));
                ctx.request_repaint();
            });
        }
        match self.bitmaps.get(&key) {
            Some(Bitmap::Ready(texture)) => Some(texture),
            _ => None,
        }
    }

    /// An image file on disk; `None` until it has loaded (or when it is missing).
    pub fn file(&mut self, ctx: &Context, path: &std::path::Path) -> Option<&TextureHandle> {
        self.receive(ctx);
        let key = format!("file:{}", path.display());
        if !self.bitmaps.contains_key(&key) {
            self.bitmaps.insert(key.clone(), Bitmap::Loading);
            let (tx, ctx, path, key) = (
                self.decoded_tx.clone(),
                ctx.clone(),
                path.to_path_buf(),
                key.clone(),
            );
            std::thread::spawn(move || {
                let image = std::fs::read(path)
                    .ok()
                    .and_then(|bytes| decode(&bytes).ok());
                let _ = tx.send((key, image));
                ctx.request_repaint();
            });
        }
        match self.bitmaps.get(&key) {
            Some(Bitmap::Ready(texture)) => Some(texture),
            _ => None,
        }
    }

    fn receive(&mut self, ctx: &Context) {
        while let Ok((name, image)) = self.decoded_rx.try_recv() {
            let bitmap = match image {
                Some(image) => {
                    Bitmap::Ready(ctx.load_texture(&name, image, TextureOptions::LINEAR))
                }
                None => Bitmap::Missing,
            };
            self.bitmaps.insert(name, bitmap);
        }
    }

    /// A white icon from WPF path data, stretched uniformly into `size` points (WPF
    /// `Stretch="Uniform"`), to be tinted when drawn.
    pub fn icon(
        &mut self,
        ctx: &Context,
        path_data: &'static str,
        size: Vec2,
    ) -> Option<TextureHandle> {
        let ppp = ctx.pixels_per_point();
        let (w, h) = (
            (size.x * ppp).round().max(1.0) as u32,
            (size.y * ppp).round().max(1.0) as u32,
        );
        let key = (path_data.as_ptr() as usize, w, h);
        if let Some(texture) = self.icons.get(&key) {
            return Some(texture.clone());
        }
        let image = rasterise_path(path_data, w, h)?;
        let texture = ctx.load_texture(format!("icon-{key:?}"), image, TextureOptions::LINEAR);
        self.icons.insert(key, texture.clone());
        Some(texture)
    }

    /// Draws an icon centred in `rect` at `size`.
    pub fn paint_icon(
        &mut self,
        ui: &egui::Ui,
        path_data: &'static str,
        rect: Rect,
        size: Vec2,
        tint: Color32,
    ) {
        if let Some(texture) = self.icon(ui.ctx(), path_data, size) {
            let icon_rect = Rect::from_center_size(rect.center(), size);
            ui.painter()
                .image(texture.id(), snap(ui, icon_rect), FULL_UV, tint);
        }
    }
}

pub const FULL_UV: Rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));

/// `rect` moved onto the physical pixel grid, so textures drawn 1:1 stay sharp.
pub fn snap(ui: &egui::Ui, rect: Rect) -> Rect {
    let ppp = ui.ctx().pixels_per_point();
    let min = (rect.min.to_vec2() * ppp).round() / ppp;
    Rect::from_min_size(min.to_pos2(), rect.size())
}

fn decode(bytes: &[u8]) -> image::ImageResult<ColorImage> {
    let rgba = image::load_from_memory(bytes)?.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Ok(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

/// WPF path data rendered white into a `w` × `h` pixel image, scaled uniformly and centred.
fn rasterise_path(data: &str, w: u32, h: u32) -> Option<ColorImage> {
    let data = data.trim();
    // WPF: `F0` = even-odd (the default), `F1` = non-zero.
    let (rule, d) = match data.strip_prefix("F1") {
        Some(rest) => ("nonzero", rest),
        None => ("evenodd", data.strip_prefix("F0").unwrap_or(data)),
    };
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><path fill="white" fill-rule="{rule}" d="{d}"/></svg>"#
    );
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).ok()?;
    let bounds = tree.root().abs_bounding_box();
    let scale = (w as f32 / bounds.width()).min(h as f32 / bounds.height());
    let dx = (w as f32 - bounds.width() * scale) / 2.0 - bounds.x() * scale;
    let dy = (h as f32 - bounds.height() * scale) / 2.0 - bounds.y() * scale;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, dx, dy),
        &mut pixmap.as_mut(),
    );
    Some(ColorImage::from_rgba_premultiplied(
        [w as usize, h as usize],
        pixmap.data(),
    ))
}

/// The bytes at `url`, from the disk cache when present.
fn fetch_cached(url: &str) -> Option<Vec<u8>> {
    let dir = crate::platform::cache_dir()?;
    // FNV-1a: a file name that stays the same across builds.
    let hash = url.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    let path = dir.join(format!("{hash:016x}"));
    if let Ok(bytes) = std::fs::read(&path) {
        return Some(bytes);
    }
    let mut response = ureq::get(url)
        .header("User-Agent", "Arma3Launcher/1.0")
        .call()
        .ok()?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(32 * 1024 * 1024)
        .read_to_vec()
        .ok()?;
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(&path, &bytes);
    Some(bytes)
}
