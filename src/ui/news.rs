//! The News page (`Views/News/News.xaml`, `NewsItemTemplate`): reports as expandable rows with
//! the post rendered in place, "Mark all as read" and "LOAD MORE".

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Id, Rect, Sense, UiBuilder, pos2, vec2};

use super::gfx::{FULL_UV, Gfx, snap};
use super::widgets::{ToolbarIcon, tab_control, toolbar, toolbar_button};
use super::{icons, rich, theme};
use crate::news::html::{self, Block};
use crate::news::{self, Post};

const HEADER_PADDING: [f32; 4] = [5.0, 10.0, 15.0, 10.0];

enum Loaded {
    Posts {
        posts: Vec<Post>,
        page: u32,
        fetched: usize,
    },
    Failed(String),
}

#[derive(PartialEq)]
enum MoreState {
    Idle,
    Receiving,
    Failed,
    Exhausted,
}

pub struct NewsPage {
    pub posts: Vec<Post>,
    /// Posts newer than the newest one seen before (`IsNew`).
    pub unread: HashSet<u64>,
    expanded: HashSet<u64>,
    bodies: HashMap<u64, Vec<Block>>,
    page: u32,
    more: MoreState,
    launcher_data: PathBuf,
    tx: Sender<Loaded>,
    rx: Receiver<Loaded>,
}

impl NewsPage {
    /// Starts with the cached posts and fetches the first page in the background; posts newer
    /// than `seen_newest` are unread.
    pub fn new(
        posts: Vec<Post>,
        launcher_data: PathBuf,
        seen_newest: Option<u64>,
        ctx: &egui::Context,
    ) -> Self {
        let (tx, rx) = channel();
        let unread = match seen_newest {
            Some(seen) => posts.iter().filter(|p| p.id > seen).map(|p| p.id).collect(),
            None => HashSet::new(),
        };
        let mut page = Self {
            posts,
            unread,
            expanded: HashSet::new(),
            bodies: HashMap::new(),
            page: 0,
            more: MoreState::Idle,
            launcher_data,
            tx,
            rx,
        };
        page.fetch(1, ctx);
        page
    }

    fn fetch(&mut self, page: u32, ctx: &egui::Context) {
        self.more = MoreState::Receiving;
        let (tx, dir, ctx) = (self.tx.clone(), self.launcher_data.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = news::fetch_page(page).and_then(|fetched| {
                let count = fetched.len();
                news::merge_into_cache(&dir, fetched)
                    .map(|posts| (posts, count))
                    .map_err(|e| e.to_string())
            });
            let _ = tx.send(match result {
                Ok((posts, fetched)) => Loaded::Posts {
                    posts,
                    page,
                    fetched,
                },
                Err(e) => Loaded::Failed(e),
            });
            ctx.request_repaint();
        });
    }

    /// The newest post id, to remember as seen.
    pub fn newest(&self) -> Option<u64> {
        self.posts.iter().map(|p| p.id).max()
    }

    /// Takes fetched posts; new ones are unread.
    pub fn update(&mut self) {
        while let Ok(loaded) = self.rx.try_recv() {
            match loaded {
                Loaded::Posts {
                    posts,
                    page,
                    fetched,
                } => {
                    let newest_before = self.newest().unwrap_or(0);
                    if page == 1 && newest_before > 0 {
                        self.unread
                            .extend(posts.iter().filter(|p| p.id > newest_before).map(|p| p.id));
                    }
                    self.posts = posts;
                    self.page = page;
                    self.more = if fetched == 0 {
                        MoreState::Exhausted
                    } else {
                        MoreState::Idle
                    };
                }
                Loaded::Failed(e) => {
                    eprintln!("arma3-launcher: news: {e}");
                    self.more = MoreState::Failed;
                }
            }
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, gfx: &mut Gfx, page: Rect) {
        let (_, content) = tab_control(ui, "news-tabs", page, &["Reports"], 0);
        let bar = Rect::from_min_size(content.min, vec2(content.width(), 40.0));
        toolbar(ui, bar);
        let mut x = bar.left();
        if toolbar_button(
            ui,
            gfx,
            Id::unique("news-mark-read"),
            &mut x,
            bar,
            ToolbarIcon::Image("markallasread.png"),
            "Mark all as read",
        )
        .clicked()
        {
            self.unread.clear();
        }

        let body = Rect::from_min_max(pos2(content.left(), bar.bottom() + 1.0), content.max);
        let mut body_ui = ui.new_child(UiBuilder::new().max_rect(body));
        body_ui.set_clip_rect(body.intersect(ui.clip_rect()));
        let mut toggle = None;
        let mut load_more = false;
        egui::ScrollArea::vertical()
            .id_salt("news-list")
            .auto_shrink([false, false])
            .show(&mut body_ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 1.0);
                let width = body.width();
                for post in &self.posts {
                    let expanded = self.expanded.contains(&post.id);
                    let unread = self.unread.contains(&post.id);
                    if item_header(ui, gfx, width, post, expanded, unread) {
                        toggle = Some(post.id);
                    }
                    if expanded {
                        let blocks = self
                            .bodies
                            .entry(post.id)
                            .or_insert_with(|| html::parse(&post.content));
                        let top = ui.cursor().min;
                        let inner = Rect::from_min_size(
                            top + vec2(56.0, 10.0),
                            vec2(width - 56.0 - 20.0, f32::INFINITY),
                        );
                        // Paint the body background under the text once its height is known.
                        let background = ui.painter().add(egui::Shape::Noop);
                        let mut body_ui = ui.new_child(UiBuilder::new().max_rect(inner));
                        rich::blocks(
                            &mut body_ui,
                            gfx,
                            Id::unique(("news-body", post.id)),
                            blocks,
                        );
                        let bottom = body_ui.min_rect().bottom() + 10.0;
                        let rect = Rect::from_min_max(top, pos2(top.x + width, bottom));
                        ui.painter().set(
                            background,
                            egui::Shape::rect_filled(rect, 0.0, theme::argb(0xFF333333)),
                        );
                        ui.allocate_rect(rect, Sense::hover());
                    }
                }
                // LOAD MORE (`DefaultButtonStyle`) with its receiving / error / exhausted states.
                let label = match self.more {
                    MoreState::Idle => "LOAD MORE",
                    MoreState::Receiving => "RECEIVING...",
                    MoreState::Failed => "ERROR LOADING, TRY AGAIN",
                    MoreState::Exhausted => "NOTHING TO SEE HERE, MOVE ALONG",
                };
                let galley = ui.painter().layout_no_wrap(
                    label.into(),
                    theme::body(theme::BODY_SIZE),
                    theme::WHITE,
                );
                let (rect, response) =
                    ui.allocate_exact_size(vec2(width, galley.size().y + 16.0), Sense::click());
                ui.painter().rect_filled(rect, 0.0, theme::BLACK);
                if response.hovered() {
                    ui.painter()
                        .rect_filled(rect, 0.0, theme::BUTTON_HOVER_OVERLAY);
                }
                ui.painter()
                    .galley(rect.center() - galley.size() / 2.0, galley, theme::WHITE);
                load_more =
                    response.clicked() && matches!(self.more, MoreState::Idle | MoreState::Failed);
            });
        if let Some(id) = toggle
            && !self.expanded.remove(&id)
        {
            self.expanded.insert(id);
            self.unread.remove(&id);
        }
        if load_more {
            let next = if self.more == MoreState::Failed && self.page == 0 {
                1
            } else {
                self.page + 1
            };
            self.fetch(next, ui.ctx());
        }
    }
}

/// The report header: type icon, title, excerpt, byline and the open-in-browser button.
/// Returns true when it was clicked (to expand or collapse).
fn item_header(
    ui: &mut egui::Ui,
    gfx: &mut Gfx,
    width: f32,
    post: &Post,
    expanded: bool,
    unread: bool,
) -> bool {
    let [left, top, right, bottom] = HEADER_PADDING;
    let title_size = theme::HEADER_SIZE;
    let height = top + 2.0 + 22.0 + 2.0 + 18.0 + 2.0 + 18.0 + 2.0 + bottom;
    let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let background = if expanded {
        theme::argb(0xFF151515)
    } else if unread {
        theme::argb(0xC0EAA32D)
    } else {
        theme::STRONG_PANEL
    };
    ui.painter().rect_filled(rect, 0.0, background);
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, 0.0, theme::BUTTON_HOVER_OVERLAY);
    }
    let inner = Rect::from_min_max(rect.min + vec2(left, top), rect.max - vec2(right, bottom));
    let icon = Rect::from_min_size(
        pos2(inner.left() + 10.0, inner.center().y - 18.0),
        vec2(36.0, 36.0),
    );
    if let Some(texture) = gfx.bitmap(ui.ctx(), post.icon()) {
        let tint = if unread && !expanded {
            theme::WHITE
        } else {
            theme::accent()
        };
        ui.painter()
            .image(texture.id(), snap(ui, icon), FULL_UV, tint);
    }

    // Open in browser: 18 × 18 glyph at the right.
    let open = Rect::from_min_size(
        pos2(inner.right() - 28.0, inner.center().y - 14.0),
        vec2(28.0, 28.0),
    );
    let open_response = ui.interact(open, Id::unique(("news-open", post.id)), Sense::click());
    if open_response.hovered() {
        ui.painter()
            .rect_filled(open, 0.0, theme::BUTTON_HOVER_OVERLAY);
    }
    gfx.paint_icon(ui, icons::OPEN_SITE, open, vec2(18.0, 18.0), theme::WHITE);
    if open_response.clicked() {
        let _ = open::that_detached(&post.url);
    }

    let (title_color, excerpt_color, meta_color) = if unread && !expanded {
        (theme::WHITE, theme::WHITE, theme::argb(0xAAFFFFFF))
    } else {
        (
            theme::accent(),
            theme::NEWS_EXCERPT,
            theme::NEWS_META.gamma_multiply(0.7),
        )
    };
    let text_left = icon.right() + 10.0;
    let text_width = open.left() - 20.0 - text_left;
    let mut y = inner.top() + 2.0;
    let title =
        ui.painter()
            .layout_no_wrap(post.title.clone(), theme::heading(title_size), title_color);
    let title_height = title.size().y;
    ui.painter().galley(pos2(text_left, y), title, title_color);
    y += title_height + 2.0;
    let mut job = egui::text::LayoutJob::simple(
        post.excerpt_text(),
        theme::body(theme::BODY_SIZE),
        excerpt_color,
        text_width,
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let excerpt = ui.painter().layout_job(job);
    let excerpt_height = excerpt.size().y;
    ui.painter()
        .galley(pos2(text_left, y), excerpt, excerpt_color);
    y += excerpt_height + 2.0;
    let meta = ui.painter().layout_no_wrap(
        post.reported_by(),
        theme::body(theme::BODY_SIZE),
        meta_color,
    );
    ui.painter().galley(pos2(text_left, y), meta, meta_color);
    response.clicked() && !open_response.hovered()
}
