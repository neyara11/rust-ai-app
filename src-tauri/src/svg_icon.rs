//! Validation for SVG icons from packages.
//!
//! SVG is a document format with active content, so an icon is never rendered
//! inline: the UI puts the validated bytes into
//! `<img src="data:image/svg+xml;base64,...">`, where scripts do not run. This
//! module is a second line of defence: it rejects documents that could become
//! dangerous if the rendering path ever changes (inline insertion, asset URL,
//! export, open-in-browser).
//!
//! The validator is intentionally strict: a rejected icon only means the card
//! falls back to the letter avatar, never a broken install.

use crate::error::{msg, Error, Result};
use quick_xml::events::attributes::Attributes;
use quick_xml::events::Event;
use quick_xml::{Reader, XmlVersion};

/// Icons are tiny by design; 128 KiB is far above any real 40x40 glyph.
pub const MAX_ICON_BYTES: u64 = 128 * 1024;
const MAX_ELEMENTS: usize = 2_000;
const MAX_DEPTH: usize = 64;
const MAX_TEXT_BYTES: usize = 96 * 1024;

/// Elements a package icon may use. Everything that can carry script, foreign
/// HTML, remote resources or animation is deliberately absent.
const ALLOWED_ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "path",
    "rect",
    "circle",
    "ellipse",
    "line",
    "polyline",
    "polygon",
    "text",
    "tspan",
    "defs",
    "lineargradient",
    "radialgradient",
    "stop",
    "clippath",
    "mask",
    "symbol",
    "use",
    "title",
    "desc",
    "style",
];

/// Validates the SVG and returns the original UTF-8 document ready for a
/// `data:` URL. Nothing is rewritten: a document that passes contains only the
/// allow-listed shapes and safe attributes.
pub fn sanitize(input: &[u8]) -> Result<String> {
    if input.len() as u64 > MAX_ICON_BYTES {
        return msg(format!(
            "icon is larger than {} KiB",
            MAX_ICON_BYTES / 1024
        ));
    }
    let text = std::str::from_utf8(input)
        .map_err(|_| Error::Msg("icon is not UTF-8 SVG".to_string()))?;
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(false);

    let mut depth = 0usize;
    let mut elements = 0usize;
    let mut text_bytes = 0usize;
    let mut root_seen = false;
    let mut style_level: Option<usize> = None;

    loop {
        match reader.read_event() {
            Err(e) => return msg(format!("icon is not valid XML: {e}")),
            Ok(Event::Eof) => break,
            Ok(Event::Decl(_)) | Ok(Event::Comment(_)) => continue,
            Ok(Event::DocType(_)) => return msg("icon must not contain a DOCTYPE"),
            Ok(Event::PI(_)) => return msg("icon must not contain processing instructions"),
            Ok(Event::CData(_)) => return msg("icon must not contain CDATA sections"),
            Ok(Event::GeneralRef(_)) => return msg("icon must not contain entity references"),
            Ok(Event::Text(data)) => {
                text_bytes += data.len();
                if text_bytes > MAX_TEXT_BYTES {
                    return msg("icon contains too much text");
                }
                if style_level.is_some() {
                    let value = data.xml_content(XmlVersion::Implicit1_0);
                    check_css(&value)?;
                }
            }
            Ok(Event::Start(element)) => {
                let name = element_name(element.name().as_ref())?;
                elements += 1;
                if elements > MAX_ELEMENTS {
                    return msg("icon has too many elements");
                }
                if !root_seen {
                    if name != "svg" {
                        return msg("icon has no <svg> root");
                    }
                    root_seen = true;
                }
                check_element(&name, element.attributes())?;
                depth += 1;
                if depth > MAX_DEPTH {
                    return msg("icon is nested too deeply");
                }
                if name == "style" {
                    style_level = Some(depth);
                }
            }
            Ok(Event::Empty(element)) => {
                let name = element_name(element.name().as_ref())?;
                elements += 1;
                if elements > MAX_ELEMENTS {
                    return msg("icon has too many elements");
                }
                if !root_seen {
                    if name != "svg" {
                        return msg("icon has no <svg> root");
                    }
                    root_seen = true;
                }
                check_element(&name, element.attributes())?;
            }
            Ok(Event::End(_)) => {
                if style_level == Some(depth) {
                    style_level = None;
                }
                depth = depth.saturating_sub(1);
            }
        }
    }

    if !root_seen {
        return msg("icon has no <svg> root");
    }
    Ok(text.to_string())
}

fn element_name(raw: &str) -> Result<String> {
    if raw.contains(':') {
        return msg("icon elements must not use namespaces");
    }
    Ok(raw.to_ascii_lowercase())
}

fn check_element(name: &str, attributes: Attributes<'_>) -> Result<()> {
    if !ALLOWED_ELEMENTS.contains(&name) {
        return msg(format!("icon element <{name}> is not allowed"));
    }
    for attribute in attributes {
        let attribute =
            attribute.map_err(|_| Error::Msg("icon has a malformed attribute".to_string()))?;
        let key = attribute.key;
        let local = key.local_name().as_ref().to_ascii_lowercase();
        let prefix = key
            .prefix()
            .map(|prefix| prefix.as_ref().to_ascii_lowercase());
        if prefix.is_none() && local == "xmlns" {
            continue;
        }
        if prefix.as_deref() == Some("xmlns") {
            continue;
        }
        if prefix.is_some() && !(prefix.as_deref() == Some("xlink") && local == "href") {
            return msg(format!("icon attribute {local} uses an unknown namespace"));
        }
        if local.starts_with("on") {
            return msg(format!("icon event attribute {local} is not allowed"));
        }
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(|_| Error::Msg("icon attribute is not valid UTF-8".to_string()))?;
        if value.contains('<') {
            return msg("icon attribute contains markup");
        }
        check_css(&value)?;
        if local == "href" && !value.trim_start().starts_with('#') {
            return msg("icon link must reference a local fragment");
        }
    }
    Ok(())
}

/// Rejects remote/local-resource references and script-capable CSS values.
fn check_css(value: &str) -> Result<()> {
    if value.contains('\\') {
        // CSS escapes can hide `url(`, `@import` and similar markers.
        return msg("icon contains CSS escapes");
    }
    let lower = value.to_ascii_lowercase();
    for marker in ["javascript:", "vbscript:", "@import", "expression("] {
        if lower.contains(marker) {
            return msg(format!("icon contains a forbidden value ({marker})"));
        }
    }
    let mut rest = lower.as_str();
    while let Some(index) = rest.find("url(") {
        let after = &rest[index + 4..];
        if !after.trim_start().starts_with('#') {
            return msg("icon may only reference local fragments");
        }
        rest = after;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &[u8] = br##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
  <defs><linearGradient id="g"><stop offset="0" stop-color="#123"/></linearGradient></defs>
  <rect width="24" height="24" rx="6" fill="url(#g)"/>
  <path d="M6 12h12" stroke="#fff" stroke-width="2"/>
</svg>"##;

    #[test]
    fn accepts_a_normal_icon() {
        assert!(sanitize(VALID).is_ok());
    }

    #[test]
    fn rejects_active_content() {
        for (label, icon) in [
            (
                "script",
                br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#.as_slice(),
            ),
            (
                "event",
                br#"<svg xmlns="http://www.w3.org/2000/svg" onload="alert(1)"/>"#.as_slice(),
            ),
            (
                "foreignObject",
                br#"<svg xmlns="http://www.w3.org/2000/svg"><foreignObject><body xmlns="http://www.w3.org/1999/xhtml">x</body></foreignObject></svg>"#.as_slice(),
            ),
            (
                "external href",
                br##"<svg xmlns="http://www.w3.org/2000/svg"><use href="https://evil.example/a.svg#x"/></svg>"##.as_slice(),
            ),
            (
                "doctype",
                br#"<!DOCTYPE svg [<!ENTITY xxe SYSTEM "file:///c:/windows/win.ini">]><svg xmlns="http://www.w3.org/2000/svg"><text>&xxe;</text></svg>"#.as_slice(),
            ),
            (
                "remote css",
                br#"<svg xmlns="http://www.w3.org/2000/svg"><style>rect{fill:url(https://evil.example/a)}</style><rect/></svg>"#.as_slice(),
            ),
            (
                "image",
                br#"<svg xmlns="http://www.w3.org/2000/svg"><image href="data:image/svg+xml;base64,AA"/></svg>"#.as_slice(),
            ),
        ] {
            assert!(sanitize(icon).is_err(), "{label} must be rejected");
        }
    }

    #[test]
    fn rejects_oversize_and_non_svg() {
        let big = vec![b' '; MAX_ICON_BYTES as usize + 1];
        assert!(sanitize(&big).is_err());
        assert!(sanitize(b"<html/>").is_err());
    }
}
