//! Open Graph tags, inserted into the document before `</head>`.
//!
//! # Why string insertion and not an HTML parser
//!
//! Because the input is not arbitrary HTML — it is one file this repo
//! builds, and the operation is "put these five meta tags in the head". A
//! parser would add a dependency, a parse of the same bytes on every share
//! request, and a class of failure (a document that re-serializes
//! differently than it was written) in exchange for generality nobody needs.
//! If the document ever loses its `</head>`, the tags are left out and the
//! page still serves — [`inject`] cannot fail.
//!
//! # What is in a card
//!
//! The title is the project's own name from the sidecar (client-computed,
//! stored verbatim — D3 means the server never derived it), the image is the
//! preview PNG on the blob plane when the last push carried one, and the url
//! is the canonical share link. Every value is attribute-escaped: a project
//! named `"><script>` is a project name, not markup.

use std::fmt::Write as _;

/// The tags a share card is made of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OgTags {
    /// `og:title` — the project's display name.
    pub title: String,
    /// `og:description` — one line about what the link opens.
    pub description: String,
    /// `og:image` — absolute URL of the preview PNG, when there is one.
    pub image: Option<String>,
    /// `og:url` — the canonical share link.
    pub url: String,
}

impl OgTags {
    /// The markup for these tags, one element per line.
    pub fn to_html(&self) -> String {
        let mut html = String::new();
        let _ = writeln!(html, r#"<meta property="og:type" content="website">"#);
        let _ = writeln!(
            html,
            r#"<meta property="og:site_name" content="LightPlayer">"#
        );
        let _ = writeln!(
            html,
            r#"<meta property="og:title" content="{}">"#,
            escape_attribute(&self.title)
        );
        let _ = writeln!(
            html,
            r#"<meta property="og:description" content="{}">"#,
            escape_attribute(&self.description)
        );
        let _ = writeln!(
            html,
            r#"<meta property="og:url" content="{}">"#,
            escape_attribute(&self.url)
        );
        match &self.image {
            Some(image) => {
                let _ = writeln!(
                    html,
                    r#"<meta property="og:image" content="{}">"#,
                    escape_attribute(image)
                );
                let _ = writeln!(
                    html,
                    r#"<meta name="twitter:card" content="summary_large_image">"#
                );
            }
            None => {
                let _ = writeln!(html, r#"<meta name="twitter:card" content="summary">"#);
            }
        }
        html
    }
}

/// The document with `tags` inserted before its `</head>`.
///
/// A document with no `</head>` is returned untouched: a share link that
/// unfurls plainly is a smaller problem than one that 500s.
pub fn inject(document: &[u8], tags: &OgTags) -> Vec<u8> {
    let Some(at) = find_head_close(document) else {
        return document.to_vec();
    };
    let markup = tags.to_html();

    let mut injected = Vec::with_capacity(document.len() + markup.len());
    injected.extend_from_slice(&document[..at]);
    injected.extend_from_slice(markup.as_bytes());
    injected.extend_from_slice(&document[at..]);
    injected
}

/// Where `</head>` starts, case-insensitively.
fn find_head_close(document: &[u8]) -> Option<usize> {
    const NEEDLE: &[u8] = b"</head>";
    document
        .windows(NEEDLE.len())
        .position(|window| window.eq_ignore_ascii_case(NEEDLE))
}

/// Escape a value for use inside a double-quoted HTML attribute.
fn escape_attribute(raw: &str) -> String {
    let mut escaped = String::with_capacity(raw.len());
    for character in raw.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_land_immediately_before_the_head_close() {
        let document = b"<html><head><title>x</title></head><body></body></html>";
        let injected = String::from_utf8(inject(document, &sample())).unwrap();

        assert!(injected.contains(r#"<meta property="og:title" content="Zook Dome">"#));
        assert!(injected.contains(r#"<meta property="og:image" content="http://host/b/abc">"#));
        let head_close = injected.find("</head>").unwrap();
        let title = injected.find("og:title").unwrap();
        assert!(title < head_close, "tags must be inside the head");
        assert!(injected.ends_with("<body></body></html>"));
    }

    #[test]
    fn the_close_tag_is_matched_whatever_its_case() {
        let injected = inject(b"<head></HEAD>", &sample());
        assert!(String::from_utf8(injected).unwrap().contains("og:title"));
    }

    /// A project name is data. If it could close an attribute it could
    /// close the head, and a share card would be an injection point.
    #[test]
    fn a_hostile_name_cannot_escape_its_attribute() {
        let tags = OgTags {
            title: r#""><script>alert(1)</script>"#.to_string(),
            ..sample()
        };
        let injected = String::from_utf8(inject(b"<head></head>", &tags)).unwrap();

        assert!(!injected.contains("<script>"));
        assert!(injected.contains("&quot;&gt;&lt;script&gt;"));
    }

    #[test]
    fn a_document_without_a_head_is_served_as_it_is() {
        let document = b"<html><body>no head</body></html>";
        assert_eq!(inject(document, &sample()), document.to_vec());
    }

    /// No preview PNG is a real state (a project published but never
    /// pushed), and it must not produce an `og:image` pointing at nothing.
    #[test]
    fn without_an_image_the_card_is_a_summary() {
        let tags = OgTags {
            image: None,
            ..sample()
        };
        let html = tags.to_html();
        assert!(!html.contains("og:image"));
        assert!(html.contains(r#"content="summary""#));
    }

    fn sample() -> OgTags {
        OgTags {
            title: "Zook Dome".to_string(),
            description: "A LightPlayer project.".to_string(),
            image: Some("http://host/b/abc".to_string()),
            url: "http://host/p/zook-dome-prj_x".to_string(),
        }
    }
}
