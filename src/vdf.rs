//! Minimal reader for Steam's text KeyValues (VDF) files such as `appmanifest_107410.acf`.

/// The value at `path` (nested keys, matched ignoring ASCII case) in a VDF document.
pub fn lookup(text: &str, path: &[&str]) -> Option<String> {
    let tokens = tokenize(text);
    let mut depth = 0usize;
    let mut matched = 0usize;
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            Token::Open => depth += 1,
            Token::Close => {
                depth = depth.saturating_sub(1);
                matched = matched.min(depth);
            }
            Token::Str(key) => {
                let next = tokens.get(i + 1);
                let is_target = depth == matched
                    && matched < path.len()
                    && key.eq_ignore_ascii_case(path[matched]);
                match next {
                    Some(Token::Str(value)) => {
                        if is_target && matched + 1 == path.len() {
                            return Some(value.clone());
                        }
                        i += 1;
                    }
                    Some(Token::Open) if is_target => matched += 1,
                    _ => {}
                }
            }
        }
        i += 1;
    }
    None
}

enum Token {
    Str(String),
    Open,
    Close,
}

fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => tokens.push(Token::Open),
            '}' => tokens.push(Token::Close),
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => s.extend(chars.next()),
                        _ => s.push(c),
                    }
                }
                tokens.push(Token::Str(s));
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#""AppState"
{
	"appid"		"107410"
	"UserConfig"
	{
		"BetaKey"		"rc"
	}
	"MountedConfig"
	{
		"language"		"english"
		"BetaKey"		"profiling"
	}
}"#;

    #[test]
    fn finds_nested_values_only_under_their_parents() {
        assert_eq!(
            lookup(MANIFEST, &["AppState", "MountedConfig", "BetaKey"]).as_deref(),
            Some("profiling")
        );
        assert_eq!(
            lookup(MANIFEST, &["AppState", "UserConfig", "betakey"]).as_deref(),
            Some("rc")
        );
        assert_eq!(
            lookup(MANIFEST, &["AppState", "appid"]).as_deref(),
            Some("107410")
        );
        assert_eq!(lookup(MANIFEST, &["AppState", "Missing", "BetaKey"]), None);
    }
}
