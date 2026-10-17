//! Draws news post blocks ([`crate::news::html::Block`]) as rich text: paragraphs with bold,
//! italic and hyperlink runs, lists, images, and the YouTube and tweet cards
//! (`NewsYoutubeButton`, `NewsTwitterButton` in `NewsStyle.xaml`).

use std::sync::Arc;

use eframe::egui::{
    self, Color32, FontFamily, FontId, Galley, Id, Rect, Sense, Stroke, text::LayoutJob, vec2,
};

use super::gfx::{FULL_UV, Gfx};
use super::{icons, theme};
use crate::news::html::{Block, Span};

/// `NewsContentFontColor`.
const TEXT: Color32 = theme::argb(0xFFD3D3D3);
const LIST_INDENT: f32 = 20.0;
const PARAGRAPH_GAP: f32 = 10.0;

/// Character ranges of a laid-out paragraph and the hyperlink each belongs to.
type LinkRanges = Vec<(usize, usize, Option<String>)>;

fn font(span: &Span, size: f32) -> FontId {
    let family = match (span.bold, span.italic) {
        (true, true) => "roboto-bolditalic",
        (true, false) => "roboto-bold",
        (false, true) => "roboto-italic",
        (false, false) => return FontId::new(size, FontFamily::Proportional),
    };
    FontId::new(size, FontFamily::Name(family.into()))
}

/// Lays out `spans`; returns the galley and, per character range, the link it belongs to.
fn layout(
    ui: &egui::Ui,
    spans: &[Span],
    size: f32,
    color: Color32,
    width: f32,
    hovered_link: Option<&str>,
) -> (Arc<Galley>, LinkRanges) {
    let mut job = LayoutJob {
        wrap: egui::text::TextWrapping {
            max_width: width,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut ranges = Vec::new();
    let mut chars = 0;
    for span in spans {
        let mut format = egui::TextFormat::simple(font(span, size), color);
        if let Some(link) = &span.link {
            // Hyperlink: accent and underlined; red without underline on hover.
            let hovered = hovered_link == Some(link.as_str());
            format.color = if hovered { Color32::RED } else { theme::ACCENT };
            if !hovered {
                format.underline = Stroke::new(1.0, theme::ACCENT);
            }
        } else if span.underline {
            format.underline = Stroke::new(1.0, color);
        }
        job.append(&span.text, 0.0, format);
        let n = span.text.chars().count();
        ranges.push((chars, chars + n, span.link.clone()));
        chars += n;
    }
    (ui.painter().layout_job(job), ranges)
}

/// A paragraph that opens its hyperlinks when clicked.
fn paragraph(
    ui: &mut egui::Ui,
    id: Id,
    spans: &[Span],
    size: f32,
    color: Color32,
    width: f32,
    left: f32,
) {
    let hovered_link: Option<String> = ui.ctx().data(|d| d.get_temp(id));
    let (galley, ranges) = layout(ui, spans, size, color, width, hovered_link.as_deref());
    let (rect, response) =
        ui.allocate_exact_size(vec2(width + left, galley.size().y), Sense::click());
    let origin = rect.min + vec2(left, 0.0);
    let link_at = |pos: egui::Pos2| {
        let index = galley.cursor_from_pos(pos - origin).index.0;
        ranges
            .iter()
            .find(|(start, end, _)| index >= *start && index < *end)
            .and_then(|(_, _, link)| link.clone())
    };
    let now_hovered = response.hover_pos().and_then(link_at);
    if now_hovered.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if now_hovered != hovered_link {
        ui.ctx()
            .data_mut(|d| d.insert_temp(id, now_hovered.clone()));
        ui.ctx().request_repaint();
    }
    if response.clicked()
        && let Some(link) = response.interact_pointer_pos().and_then(link_at)
    {
        let _ = open::that_detached(link);
    }
    ui.painter().galley(origin, galley, color);
}

/// Draws `blocks` top to bottom in the available width.
pub fn blocks(ui: &mut egui::Ui, gfx: &mut Gfx, id: Id, blocks: &[Block]) {
    let width = ui.available_width();
    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
    for (i, block) in blocks.iter().enumerate() {
        let id = id.with(i);
        match block {
            Block::Text {
                spans,
                size,
                indent,
                bullet,
            } => {
                let left = *indent as f32 * LIST_INDENT;
                if let Some(bullet) = bullet {
                    let galley =
                        ui.painter()
                            .layout_no_wrap(bullet.clone(), theme::body(*size), TEXT);
                    let pos = ui.cursor().min + vec2(left - 14.0, 0.0);
                    ui.painter().galley(pos, galley, TEXT);
                }
                paragraph(ui, id, spans, *size, TEXT, (width - left).max(50.0), left);
                ui.add_space(if bullet.is_some() { 3.0 } else { PARAGRAPH_GAP });
            }
            Block::Image { url } => {
                if let Some(texture) = gfx.remote(ui.ctx(), url) {
                    let size = texture.size_vec2();
                    let scale = (width / size.x).min(1.0);
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(width, size.y * scale), Sense::hover());
                    let image = Rect::from_center_size(rect.center(), size * scale);
                    ui.painter()
                        .image(texture.id(), image, FULL_UV, Color32::WHITE);
                    ui.add_space(PARAGRAPH_GAP);
                }
            }
            Block::Video { url, thumbnail } => video(ui, gfx, id, url, thumbnail, width),
            Block::Tweet { url, spans, byline } => tweet(ui, gfx, id, url, spans, byline, width),
        }
    }
}

/// `NewsYoutubeButton`: the thumbnail (up to 600 px) and a "Play on YouTube" line on dark red,
/// brighter red on hover.
fn video(ui: &mut egui::Ui, gfx: &mut Gfx, id: Id, url: &str, thumbnail: &str, width: f32) {
    let image_size = gfx.remote(ui.ctx(), thumbnail).map(|t| t.size_vec2());
    let image_size = image_size
        .map(|s| s * (600.0f32.min(width - 10.0) / s.x).min(1.0))
        .unwrap_or(vec2(480.0, 270.0));
    let label = ui.painter().layout_no_wrap(
        "Play on YouTube".into(),
        theme::body(theme::BODY_SIZE),
        theme::WHITE,
    );
    let height = 5.0 + image_size.y + 5.0 + label.size().y + 10.0 + 5.0;
    let (row, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let card = Rect::from_min_size(
        egui::pos2(row.center().x - (image_size.x + 10.0) / 2.0, row.top()),
        vec2(image_size.x + 10.0, height),
    );
    let response = ui.interact(card, id, Sense::click());
    let fill = if response.hovered() {
        theme::argb(0xFFE62D27)
    } else {
        theme::argb(0xFF6A1815)
    };
    ui.painter().rect_filled(card, 0.0, fill);
    if let Some(texture) = gfx.remote(ui.ctx(), thumbnail) {
        let image = Rect::from_min_size(card.min + vec2(5.0, 5.0), image_size);
        ui.painter()
            .image(texture.id(), image, FULL_UV, Color32::WHITE);
    }
    let line_y = card.top() + 5.0 + image_size.y + 5.0;
    let logo = Rect::from_min_size(
        egui::pos2(card.left() + 10.0, line_y + 5.0),
        vec2(20.0, label.size().y),
    );
    gfx.paint_icon(
        ui,
        icons::YOUTUBE_LOGO,
        logo,
        vec2(20.0, 14.0),
        theme::WHITE,
    );
    ui.painter().galley(
        egui::pos2(logo.right() + 5.0, line_y + 5.0),
        label,
        theme::WHITE,
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        let _ = open::that_detached(url);
    }
    ui.add_space(PARAGRAPH_GAP);
}

/// `NewsTwitterButton`: a white card with rounded corners, the bird, the tweet and its byline;
/// Twitter blue on hover.
fn tweet(
    ui: &mut egui::Ui,
    gfx: &mut Gfx,
    id: Id,
    url: &str,
    spans: &[Span],
    byline: &str,
    width: f32,
) {
    let card_width = width.min(540.0f32.max(width * 0.8));
    let text_width = card_width - 20.0 - 30.0 - 10.0;
    let plain: Vec<Span> = spans
        .iter()
        .map(|s| Span {
            link: None,
            ..s.clone()
        })
        .collect();
    let (text, _) = layout(
        ui,
        &plain,
        theme::BODY_SIZE,
        Color32::BLACK,
        text_width,
        None,
    );
    let by = ui.painter().layout(
        byline.to_owned(),
        theme::body(12.0),
        theme::argb(0xFF555555),
        text_width,
    );
    let height = 10.0 + text.size().y + 6.0 + by.size().y + 10.0;
    let (row, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let card = Rect::from_min_size(row.min, vec2(card_width, height));
    let response = ui.interact(card, id, Sense::click());
    let (fill, ink) = if response.hovered() {
        (theme::argb(0xFF00B0ED), Color32::WHITE)
    } else {
        (Color32::WHITE, Color32::BLACK)
    };
    ui.painter().rect_filled(card, 5.0, fill);
    let bird = Rect::from_min_size(card.min + vec2(10.0, 10.0), vec2(30.0, 24.0));
    gfx.paint_icon(
        ui,
        icons::TWITTER_LOGO,
        bird,
        vec2(26.0, 22.0),
        if response.hovered() {
            Color32::WHITE
        } else {
            theme::argb(0xFF00B0ED)
        },
    );
    let text_pos = egui::pos2(bird.right() + 10.0, card.top() + 10.0);
    let by_pos = text_pos + vec2(0.0, text.size().y + 6.0);
    ui.painter().galley(text_pos, text, ink);
    ui.painter().galley(by_pos, by, ink);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        let _ = open::that_detached(url);
    }
    ui.add_space(PARAGRAPH_GAP);
}
