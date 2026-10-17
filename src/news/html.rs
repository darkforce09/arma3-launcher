//! News post HTML as rich-text blocks, following the vanilla launcher's `HtmlToXamlConverter`:
//! headings at 22/20/18/16/12/10 pt, bold/italic/underline runs, hyperlinks, disc lists, images,
//! and the special `video-wrapper` and `twitter-tweet` blocks.

use scraper::{ElementRef, Html, Node};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub link: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Text {
        spans: Vec<Span>,
        /// Font size in pixels.
        size: f32,
        /// List nesting depth.
        indent: usize,
        /// The list marker of a list item's first paragraph.
        bullet: Option<String>,
    },
    Image {
        url: String,
    },
    /// A YouTube video: its thumbnail, opening the video.
    Video {
        url: String,
        thumbnail: String,
    },
    /// An embedded tweet: its text and byline, opening the tweet.
    Tweet {
        url: String,
        spans: Vec<Span>,
        byline: String,
    },
}

/// Body text size: 14 px.
pub const BODY_SIZE: f32 = 14.0;

/// Heading sizes in points, as WPF pixels (pt × 96 / 72).
fn heading_size(tag: &str) -> Option<f32> {
    let points = match tag {
        "h1" => 22.0,
        "h2" => 20.0,
        "h3" => 18.0,
        "h4" => 16.0,
        "h5" => 12.0,
        "h6" => 10.0,
        _ => return None,
    };
    Some(points * 96.0 / 72.0)
}

#[derive(Clone, Default)]
struct Style {
    bold: bool,
    italic: bool,
    underline: bool,
    link: Option<String>,
    size: Option<f32>,
    indent: usize,
}

struct Builder {
    blocks: Vec<Block>,
    spans: Vec<Span>,
    size: f32,
    indent: usize,
    bullet: Option<String>,
}

impl Builder {
    fn flush(&mut self) {
        // Trim leading and trailing whitespace of the paragraph.
        while self
            .spans
            .first()
            .is_some_and(|s| s.text.trim().is_empty() && s.text != "\n")
        {
            self.spans.remove(0);
        }
        if let Some(first) = self.spans.first_mut() {
            first.text = first.text.trim_start_matches(' ').to_owned();
        }
        while self.spans.last().is_some_and(|s| s.text.trim().is_empty()) {
            self.spans.pop();
        }
        if let Some(last) = self.spans.last_mut() {
            last.text = last.text.trim_end().to_owned();
        }
        if !self.spans.is_empty() {
            self.blocks.push(Block::Text {
                spans: std::mem::take(&mut self.spans),
                size: self.size,
                indent: self.indent,
                bullet: self.bullet.take(),
            });
        }
        self.bullet = None;
    }

    fn push_text(&mut self, text: &str, style: &Style) {
        // HTML whitespace collapsing.
        let mut collapsed = String::with_capacity(text.len());
        let mut last_space = self
            .spans
            .last()
            .is_none_or(|s| s.text.ends_with(' ') || s.text.ends_with('\n'));
        for c in text.chars() {
            if c.is_whitespace() {
                if !last_space {
                    collapsed.push(' ');
                    last_space = true;
                }
            } else {
                collapsed.push(c);
                last_space = false;
            }
        }
        if collapsed.is_empty() {
            return;
        }
        let span = Span {
            text: collapsed,
            bold: style.bold,
            italic: style.italic,
            underline: style.underline,
            link: style.link.clone(),
        };
        match self.spans.last_mut() {
            Some(last)
                if last.bold == span.bold
                    && last.italic == span.italic
                    && last.underline == span.underline
                    && last.link == span.link =>
            {
                last.text += &span.text;
            }
            _ => self.spans.push(span),
        }
    }

    fn start_paragraph(&mut self, style: &Style) {
        self.flush();
        self.size = style.size.unwrap_or(BODY_SIZE);
        self.indent = style.indent;
    }
}

pub fn parse(html: &str) -> Vec<Block> {
    let document = Html::parse_fragment(html);
    let mut builder = Builder {
        blocks: Vec::new(),
        spans: Vec::new(),
        size: BODY_SIZE,
        indent: 0,
        bullet: None,
    };
    walk(document.root_element(), &Style::default(), &mut builder);
    builder.flush();
    builder.blocks
}

fn has_class(element: &ElementRef, class: &str) -> bool {
    element
        .value()
        .attr("class")
        .is_some_and(|c| c.split_whitespace().any(|x| x == class))
}

fn walk(element: ElementRef, style: &Style, b: &mut Builder) {
    for child in element.children() {
        match child.value() {
            Node::Text(text) => b.push_text(text, style),
            Node::Element(_) => {
                if let Some(child) = ElementRef::wrap(child) {
                    element_block(child, style, b);
                }
            }
            _ => {}
        }
    }
}

fn element_block(e: ElementRef, style: &Style, b: &mut Builder) {
    let tag = e.value().name().to_ascii_lowercase();
    let mut inner = style.clone();
    match tag.as_str() {
        "script" | "style" | "head" | "title" | "meta" => {}
        "br" => b.spans.push(Span {
            text: "\n".into(),
            ..Default::default()
        }),
        "hr" => {
            b.start_paragraph(style);
            b.push_text("----------------------", style);
            b.flush();
        }
        "div" if has_class(&e, "video-wrapper") => {
            let link = e
                .select(&selector("a"))
                .find_map(|a| a.value().attr("href"))
                .unwrap_or_default();
            let thumbnail = e
                .select(&selector("img"))
                .find_map(|i| i.value().attr("src"))
                .unwrap_or_default();
            b.flush();
            b.blocks.push(Block::Video {
                url: link.to_owned(),
                thumbnail: thumbnail.to_owned(),
            });
        }
        "blockquote" if has_class(&e, "twitter-tweet") => {
            b.flush();
            let mut tweet = Builder {
                blocks: Vec::new(),
                spans: Vec::new(),
                size: BODY_SIZE,
                indent: 0,
                bullet: None,
            };
            if let Some(p) = e.select(&selector("p")).next() {
                walk(p, &Style::default(), &mut tweet);
            }
            tweet.flush();
            let spans = tweet.blocks.into_iter().find_map(|block| match block {
                Block::Text { spans, .. } => Some(spans),
                _ => None,
            });
            // The byline is the text after the tweet paragraph: "— Author (@handle) date".
            let all: String = e.text().collect();
            let paragraph: String = e
                .select(&selector("p"))
                .next()
                .map(|p| p.text().collect())
                .unwrap_or_default();
            let byline = all
                .replacen(&paragraph, "", 1)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .trim_start_matches('—')
                .trim()
                .to_owned();
            let url = e
                .select(&selector("a"))
                .filter_map(|a| a.value().attr("href"))
                .last()
                .unwrap_or_default();
            b.blocks.push(Block::Tweet {
                url: url.to_owned(),
                spans: spans.unwrap_or_default(),
                byline,
            });
        }
        "img" => {
            if let Some(src) = e.value().attr("src") {
                b.flush();
                b.blocks.push(Block::Image {
                    url: src.to_owned(),
                });
            }
        }
        "ul" | "ol" | "menu" | "dir" => {
            b.flush();
            inner.indent += 1;
            let mut number = 0;
            for li in e.children().filter_map(ElementRef::wrap) {
                if li.value().name().eq_ignore_ascii_case("li") {
                    number += 1;
                    b.start_paragraph(&inner);
                    b.bullet = Some(if tag == "ol" {
                        format!("{number}.")
                    } else {
                        "\u{2022}".into()
                    });
                    walk(li, &inner, b);
                    b.flush();
                } else {
                    element_block(li, &inner, b);
                }
            }
        }
        "p" | "div" | "blockquote" | "center" | "pre" | "section" | "article" | "h1" | "h2"
        | "h3" | "h4" | "h5" | "h6" | "dd" | "dl" | "dt" | "table" | "tr" => {
            if let Some(size) = heading_size(&tag) {
                inner.size = Some(size);
            }
            let bullet = b.bullet.take();
            if !b.spans.iter().all(|s| s.text.trim().is_empty()) || bullet.is_none() {
                b.start_paragraph(&inner);
            } else {
                b.size = inner.size.unwrap_or(BODY_SIZE);
            }
            b.bullet = bullet;
            walk(e, &inner, b);
            b.flush();
        }
        _ => {
            match tag.as_str() {
                "b" | "strong" | "bold" | "dfn" => inner.bold = true,
                "i" | "em" | "italic" => inner.italic = true,
                "u" | "underline" => inner.underline = true,
                "a" => inner.link = e.value().attr("href").map(str::to_owned),
                _ => {}
            }
            walk(e, &inner, b);
        }
    }
}

fn selector(s: &str) -> scraper::Selector {
    scraper::Selector::parse(s).expect("static selector")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(blocks: &[Block]) -> Vec<(String, f32, usize, Option<String>)> {
        blocks
            .iter()
            .filter_map(|b| match b {
                Block::Text {
                    spans,
                    size,
                    indent,
                    bullet,
                } => Some((
                    spans.iter().map(|s| s.text.as_str()).collect(),
                    *size,
                    *indent,
                    bullet.clone(),
                )),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn converts_a_real_post_shape() {
        let html = r#"<p>FROM: High Command<br>TO: Arma 3 Users<br></p> <h1>NOTES</h1> <ul> <li>Contact has 2 packs: <ul> <li>Contact (via <i>PLAY CONTACT</i>)</li> </ul></li> <li>See the <a href="https://dev.arma3.com/">Dev Hub</a>.</li> </ul>"#;
        let blocks = parse(html);
        let t = texts(&blocks);
        assert_eq!(t[0].0, "FROM: High Command\nTO: Arma 3 Users");
        assert_eq!((t[1].0.as_str(), t[1].1), ("NOTES", 22.0 * 96.0 / 72.0));
        assert_eq!(
            (t[2].0.as_str(), t[2].2, t[2].3.as_deref()),
            ("Contact has 2 packs:", 1, Some("\u{2022}"))
        );
        assert_eq!((t[3].0.as_str(), t[3].2), ("Contact (via PLAY CONTACT)", 2));
        assert_eq!(t[4].0, "See the Dev Hub.");
        let Block::Text { spans, .. } = &blocks[4] else {
            panic!()
        };
        assert_eq!(spans[1].link.as_deref(), Some("https://dev.arma3.com/"));
        let Block::Text { spans, .. } = &blocks[3] else {
            panic!()
        };
        assert!(spans[1].italic);
    }

    #[test]
    fn recognises_videos_images_and_tweets() {
        let html = r#"<div class="video-wrapper"><img src="https://img.youtube.com/vi/x/0.jpg"><div><a href="https://youtu.be/x" class="btn btn-play"></a></div></div><img src="https://dev.arma3.com/a.png" class="img-responsive"><blockquote class="twitter-tweet"><p>Hello <a href="https://t.co/1">#Arma3</a></p>— Arma (@Arma3official) <a href="https://twitter.com/Arma3official/status/1">May 1, 2026</a></blockquote>"#;
        let blocks = parse(html);
        assert_eq!(
            blocks[0],
            Block::Video {
                url: "https://youtu.be/x".into(),
                thumbnail: "https://img.youtube.com/vi/x/0.jpg".into()
            }
        );
        assert_eq!(
            blocks[1],
            Block::Image {
                url: "https://dev.arma3.com/a.png".into()
            }
        );
        let Block::Tweet { url, spans, byline } = &blocks[2] else {
            panic!("{blocks:?}")
        };
        assert_eq!(url, "https://twitter.com/Arma3official/status/1");
        assert_eq!(
            spans.iter().map(|s| s.text.as_str()).collect::<String>(),
            "Hello #Arma3"
        );
        assert_eq!(byline, "Arma (@Arma3official) May 1, 2026");
    }
}
