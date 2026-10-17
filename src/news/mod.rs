//! Dev-branch news posts ("reports"): the dev.arma3.com feed and the vanilla launcher's
//! `News.json` cache (`NewsViewModel`, `LauncherNewsClient`).

pub mod html;

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The feed (`LauncherNewsUrlVariableCount`): page from 1, 100 posts a page.
const FEED_URL: &str = "https://dev.arma3.com/loadDev?count=100&page={page}&absolute=1";
const USER_AGENT: &str = "Arma3Launcher/1.0";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct NewsCache {
    #[serde(default)]
    date_written: String,
    posts: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Post {
    #[serde(deserialize_with = "number_or_string")]
    pub id: u64,
    pub title: String,
    pub author: String,
    /// HTML.
    pub excerpt: String,
    /// HTML.
    #[serde(default)]
    pub content: String,
    /// `yyyy-MM-dd HH:mm:ss`.
    pub public_date: String,
    /// `sitrep`, `spotrep`, `oprep` or `techrep`.
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
}

fn number_or_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    match serde_json::Value::deserialize(d)? {
        serde_json::Value::Number(n) => {
            n.as_u64().ok_or_else(|| serde::de::Error::custom("bad id"))
        }
        serde_json::Value::String(s) => s.parse().map_err(serde::de::Error::custom),
        _ => Err(serde::de::Error::custom("bad id")),
    }
}

impl Post {
    /// The excerpt as plain text (the vanilla launcher strips its HTML).
    pub fn excerpt_text(&self) -> String {
        let fragment = scraper::Html::parse_fragment(&self.excerpt);
        let text: String = fragment.root_element().text().collect();
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// `ReportedBy` in `DeveloperNewsPostViewModel`.
    pub fn reported_by(&self) -> String {
        format!("reported by {} on {}", self.author, self.public_date)
    }

    /// The report-type icon (`NewsIcon_*36.png`), used as a mask over the accent colour.
    pub fn icon(&self) -> &'static str {
        match self.kind.as_str() {
            "sitrep" => "newsicon_sitrep36.png",
            "oprep" => "newsicon_oprep36.png",
            "techrep" => "newsicon_techrep.png",
            _ => "newsicon_spotrep36.png",
        }
    }
}

fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

fn read_cache(launcher_data: &Path) -> Option<NewsCache> {
    serde_json::from_slice(strip_bom(
        &std::fs::read(launcher_data.join("News.json")).ok()?,
    ))
    .ok()
}

/// The cached posts, newest first.
pub fn cached_posts(launcher_data: &Path) -> Vec<Post> {
    let mut posts: Vec<Post> = read_cache(launcher_data)
        .map(|c| {
            c.posts
                .into_iter()
                .filter_map(|p| serde_json::from_value(p).ok())
                .collect()
        })
        .unwrap_or_default();
    posts.sort_by_key(|p| std::cmp::Reverse(p.id));
    posts
}

/// One page of the feed, as raw JSON posts.
pub fn fetch_page(page: u32) -> Result<Vec<serde_json::Value>, String> {
    let url = FEED_URL.replace("{page}", &page.to_string());
    ureq::get(&url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_json::<Vec<serde_json::Value>>()
        .map_err(|e| e.to_string())
}

/// Merges fetched posts into `News.json` by id (newest first) and returns all posts.
pub fn merge_into_cache(
    launcher_data: &Path,
    fetched: Vec<serde_json::Value>,
) -> io::Result<Vec<Post>> {
    let mut raw = read_cache(launcher_data)
        .map(|c| c.posts)
        .unwrap_or_default();
    let id_of = |v: &serde_json::Value| -> Option<u64> {
        match v.get("id")? {
            serde_json::Value::Number(n) => n.as_u64(),
            serde_json::Value::String(s) => s.parse().ok(),
            _ => None,
        }
    };
    for post in fetched {
        let Some(id) = id_of(&post) else { continue };
        match raw.iter_mut().find(|p| id_of(p) == Some(id)) {
            Some(existing) => *existing = post,
            None => raw.push(post),
        }
    }
    raw.sort_by_key(|p| std::cmp::Reverse(id_of(p).unwrap_or(0)));
    let cache = NewsCache {
        date_written: wcf_date_now(),
        posts: raw,
    };
    let json = serde_json::to_string(&cache)?.replace('/', "\\/");
    std::fs::write(launcher_data.join("News.json"), json)?;
    Ok(cached_posts(launcher_data))
}

/// `"\/Date(<ms>+hhmm)\/"` for the current local time, as DataContractJsonSerializer writes a
/// local `DateTime` (the slashes are escaped with the rest of the document).
fn wcf_date_now() -> String {
    let now = chrono::Local::now();
    format!("/Date({}{})/", now.timestamp_millis(), now.format("%z"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_vanilla_cache_shape() {
        let json = br#"{"DateWritten":"x","Posts":[{"author":"A","content":"<p>c</p>","excerpt":"<p>Game Mini-Update 2.22<br></p>","public_date":"2026-09-30 13:37:00","title":"SPOTREP #00122","type":"spotrep","url":"https://dev.arma3.com/post/spotrep-00122","id":485}]}"#;
        let cache: NewsCache = serde_json::from_slice(json).unwrap();
        let post: Post = serde_json::from_value(cache.posts[0].clone()).unwrap();
        assert_eq!(post.id, 485);
        assert_eq!(post.excerpt_text(), "Game Mini-Update 2.22");
        assert_eq!(post.reported_by(), "reported by A on 2026-09-30 13:37:00");
        assert_eq!(post.icon(), "newsicon_spotrep36.png");
    }

    #[test]
    fn merges_by_id_newest_first() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp/news");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let post = |id: u64, title: &str| serde_json::json!({"id": id, "title": title, "author": "A", "excerpt": "", "content": "", "public_date": "", "type": "sitrep", "url": "https://x/y"});
        merge_into_cache(&dir, vec![post(1, "one"), post(3, "three")]).unwrap();
        let posts = merge_into_cache(&dir, vec![post(2, "two"), post(3, "THREE")]).unwrap();
        let titles: Vec<&str> = posts.iter().map(|p| p.title.as_str()).collect();
        assert_eq!(titles, ["THREE", "two", "one"]);
        assert!(
            std::fs::read_to_string(dir.join("News.json"))
                .unwrap()
                .contains(r"https:\/\/x\/y")
        );
    }
}
