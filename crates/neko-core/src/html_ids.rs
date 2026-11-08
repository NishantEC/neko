//! Stable `data-neko-id`s for html artifacts, and where Neko's bridge goes
//! (`docs/superpowers/specs/2026-11-08-everything-renders-design.md`, B3;
//! after Open Design's `annotateMissingOdIds`).
//!
//! Before an html version is served, every element that has neither an `id`
//! nor a `data-neko-id` gets `data-neko-id="n<8 hex>"`, so a comment names an
//! element and not only a selector that the next version may break. The id
//! is a hash of the element itself (its tag, its attributes sorted, and the
//! first 512 bytes of its text with whitespace collapsed), not of where it
//! sits: an element left unchanged in the next version keeps its id even when
//! siblings are added before it. Identical elements are told apart by their
//! order (`n1a2b3c4d`, `n1a2b3c4d-2`, …).
//!
//! The document is read by a small tokenizer, not regular expressions, that
//! ends things where a browser does: comments (`<!-->`, `<!--->` and `--!>`
//! included), doctypes, processing instructions and bogus comments are
//! skipped whole, CDATA only inside svg and math; the contents of `script`
//! (following its escaped states), `style`, `textarea`, `title` and the other
//! raw text elements are never looked into or changed; quoted attribute
//! values may hold `<` and `>`. The only change is the inserted attribute, right
//! after the tag name, plus the two bridge scripts; no newline is added, so
//! line numbers in an error match the agent's own file. Elements in `head`,
//! `html`, `body` and void elements such as `br` are not stamped.
use std::collections::{HashMap, HashSet};

/// Elements whose contents are text, read to their end tag and left alone.
const RAW_TEXT: &[&str] = &["script", "style", "textarea", "title", "xmp", "iframe", "noembed", "noframes", "noscript", "plaintext"];
/// Elements with no end tag.
const VOID: &[&str] = &["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr", "keygen"];
/// Elements never stamped: the document's frame, and what is not drawn.
const UNSTAMPED: &[&str] = &["html", "head", "body", "meta", "link", "base", "title", "script", "style", "noscript", "template", "br", "wbr"];
/// What may sit in `head`; any other start tag there begins the body.
const HEAD_CONTENT: &[&str] = &["head", "html", "base", "link", "meta", "title", "style", "script", "noscript", "template"];
/// A start tag of one of these closes an open element of the same group (`<li>` after `<li>`).
const IMPLIED_END: &[(&str, &[&str])] = &[
    ("li", &["li"]), ("p", &["p"]), ("dt", &["dt", "dd"]), ("dd", &["dt", "dd"]), ("tr", &["tr"]),
    ("td", &["td", "th"]), ("th", &["td", "th"]), ("option", &["option"]),
];
const TEXT_BYTES: usize = 512;
pub const ATTRIBUTE: &str = "data-neko-id";

struct Element {
    tag: String,
    /// Byte offset just after the tag name, where the attribute goes.
    insert_at: usize,
    stamp: bool,
    /// Its attributes, sorted, without `data-neko-id`.
    attributes: String,
    text: String,
}

/// What one reading of the document found.
struct Reading {
    elements: Vec<Element>,
    taken: HashSet<String>,
    /// Just after the first `<head …>`, `<html …>` or doctype: where an early script goes.
    early: usize,
    /// The last real `</body>`.
    body_end: Option<usize>,
}

fn starts_with_ignore_case(bytes: &[u8], at: usize, needle: &[u8]) -> bool {
    bytes.len() >= at + needle.len() && bytes[at..at + needle.len()].eq_ignore_ascii_case(needle)
}

fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from > bytes.len() { return None; }
    bytes[from..].windows(needle.len()).position(|w| w == needle).map(|i| from + i)
}

/// Where `</name` (any case, then whitespace, `/` or `>`) starts, from `from`.
fn raw_text_end(bytes: &[u8], from: usize, name: &str) -> Option<usize> {
    let mut at = from;
    while let Some(i) = find(bytes, at, b"</") {
        let after = i + 2 + name.len();
        if starts_with_ignore_case(bytes, i + 2, name.as_bytes()) && bytes.get(after).is_none_or(|b| b.is_ascii_whitespace() || *b == b'/' || *b == b'>') {
            return Some(i);
        }
        at = i + 2;
    }
    None
}

/// The offset after a comment whose `<!--` ended just before `from`, as the
/// HTML tokenizer reads it: `<!-->` and `<!--->` are whole comments, and
/// `-->` or `--!>` ends one; unterminated, it runs to the end.
fn comment_end(bytes: &[u8], from: usize) -> usize {
    if bytes.get(from) == Some(&b'>') { return from + 1; }
    if bytes[from.min(bytes.len())..].starts_with(b"->") { return from + 2; }
    let mut at = from;
    while let Some(dash) = find(bytes, at, b"--") {
        if bytes.get(dash + 2) == Some(&b'>') { return dash + 3; }
        if bytes[dash + 2..].starts_with(b"!>") { return dash + 4; }
        at = dash + 1;
    }
    bytes.len()
}

/// Whether `<name` or `</name` at `at` (`slash` says which) is a tag of
/// `name`: followed by whitespace, `/` or `>`.
fn tag_at(bytes: &[u8], at: usize, name: &str, slash: bool) -> bool {
    let open = if slash { b"</".as_slice() } else { b"<".as_slice() };
    let after = at + open.len() + name.len();
    bytes[at.min(bytes.len())..].starts_with(open)
        && starts_with_ignore_case(bytes, at + open.len(), name.as_bytes())
        && bytes.get(after).is_some_and(|b| b.is_ascii_whitespace() || *b == b'/' || *b == b'>')
}

/// Where a script's `</script` starts, following the tokenizer's script data
/// states: after `<!--` a `<script` opens a nested ("double escaped") part in
/// which `</script>` does not end the script, and `-->` leaves both.
fn script_end(bytes: &[u8], from: usize) -> Option<usize> {
    #[derive(PartialEq)]
    enum State { Data, Escaped, DoubleEscaped }
    let mut state = State::Data;
    let mut i = from;
    while i < bytes.len() {
        match state {
            State::Data => {
                if bytes[i..].starts_with(b"<!--") {
                    // `<!-->` and `<!--->` open and close at once.
                    if bytes.get(i + 4) == Some(&b'>') { i += 5; continue; }
                    if bytes[i + 4..].starts_with(b"->") { i += 6; continue; }
                    state = State::Escaped;
                    i += 4;
                    continue;
                }
                if tag_at(bytes, i, "script", true) { return Some(i); }
            }
            State::Escaped => {
                if bytes[i..].starts_with(b"-->") { state = State::Data; i += 3; continue; }
                if tag_at(bytes, i, "script", true) { return Some(i); }
                if tag_at(bytes, i, "script", false) { state = State::DoubleEscaped; i += 7; continue; }
            }
            State::DoubleEscaped => {
                if bytes[i..].starts_with(b"-->") { state = State::Data; i += 3; continue; }
                if tag_at(bytes, i, "script", true) { state = State::Escaped; i += 8; continue; }
            }
        }
        i += 1;
    }
    None
}

fn is_name_byte(b: u8) -> bool {
    !(b.is_ascii_whitespace() || b == b'/' || b == b'>')
}

/// A start tag from `<` at `at`: its name, attributes, whether it closed
/// itself and the offset after its `>`. None when it never ends.
fn start_tag(bytes: &[u8], at: usize) -> Option<(String, Vec<(String, String)>, bool, usize)> {
    let mut i = at + 1;
    let name_start = i;
    while i < bytes.len() && is_name_byte(bytes[i]) { i += 1; }
    let name = String::from_utf8_lossy(&bytes[name_start..i]).to_ascii_lowercase();
    let mut attributes = Vec::new();
    let mut self_closing = false;
    loop {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
        match bytes.get(i)? {
            b'>' => return Some((name, attributes, self_closing, i + 1)),
            b'/' => { self_closing = true; i += 1; continue; }
            _ => {}
        }
        self_closing = false;
        let attr_start = i;
        // An attribute name may begin with `=` (the spec allows it); never with `>` or `/`.
        i += 1;
        while i < bytes.len() && !(bytes[i].is_ascii_whitespace() || matches!(bytes[i], b'/' | b'>' | b'=')) { i += 1; }
        let attr = String::from_utf8_lossy(&bytes[attr_start..i]).to_ascii_lowercase();
        let mut j = i;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() { j += 1; }
        let mut value = String::new();
        if bytes.get(j) == Some(&b'=') {
            j += 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() { j += 1; }
            match bytes.get(j)? {
                quote @ (b'"' | b'\'') => {
                    let end = find(bytes, j + 1, &[*quote])?;
                    value = String::from_utf8_lossy(&bytes[j + 1..end]).into_owned();
                    j = end + 1;
                }
                _ => {
                    let start = j;
                    while j < bytes.len() && !(bytes[j].is_ascii_whitespace() || bytes[j] == b'>') { j += 1; }
                    value = String::from_utf8_lossy(&bytes[start..j]).into_owned();
                }
            }
            i = j;
        }
        attributes.push((attr, value));
    }
}

fn add_text(stack: &[usize], elements: &mut [Element], text: &[u8]) {
    let text = String::from_utf8_lossy(text);
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() { return; }
    // Innermost first: an outer element holds at least as much text as an
    // inner one, so once one is full every element outside it is too.
    for &index in stack.iter().rev() {
        let held = &mut elements[index].text;
        if held.len() >= TEXT_BYTES { break; }
        for word in &words {
            if held.len() >= TEXT_BYTES { break; }
            if !held.is_empty() { held.push(' '); }
            held.push_str(word);
        }
    }
}

fn read(html: &str) -> Reading {
    let bytes = html.as_bytes();
    let mut elements: Vec<Element> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut taken = HashSet::new();
    let (mut head, mut root, mut doctype, mut body_end) = (None, None, None, None);
    let mut in_head = false;
    let mut i = 0;
    let mut text_from = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' { i += 1; continue; }
        let next = bytes.get(i + 1).copied().unwrap_or(0);
        let foreign = stack.iter().any(|&e| matches!(elements[e].tag.as_str(), "svg" | "math"));
        let markup_end = if bytes[i..].starts_with(b"<!--") {
            Some(comment_end(bytes, i + 4))
        } else if foreign && bytes[i..].starts_with(b"<![CDATA[") {
            // CDATA exists only inside svg and math; elsewhere it is a bogus comment, ended by `>`.
            Some(find(bytes, i + 9, b"]]>").map_or(bytes.len(), |e| e + 3))
        } else if next == b'!' || next == b'?' {
            let end = find(bytes, i, b">").map_or(bytes.len(), |e| e + 1);
            if doctype.is_none() && starts_with_ignore_case(bytes, i, b"<!doctype") { doctype = Some(end); }
            Some(end)
        } else {
            None
        };
        if let Some(end) = markup_end {
            add_text(&stack, &mut elements, &bytes[text_from..i]);
            i = end;
            text_from = i;
            continue;
        }
        if next == b'/' && bytes.get(i + 2).is_some_and(u8::is_ascii_alphabetic) {
            add_text(&stack, &mut elements, &bytes[text_from..i]);
            let mut j = i + 2;
            while j < bytes.len() && is_name_byte(bytes[j]) { j += 1; }
            let name = String::from_utf8_lossy(&bytes[i + 2..j]).to_ascii_lowercase();
            if name == "body" { body_end = Some(i); }
            if name == "head" { in_head = false; }
            if let Some(depth) = stack.iter().rposition(|&e| elements[e].tag == name) { stack.truncate(depth); }
            i = find(bytes, j, b">").map_or(bytes.len(), |e| e + 1);
            text_from = i;
            continue;
        }
        if !next.is_ascii_alphabetic() { i += 1; continue; }
        let Some((name, attributes, self_closing, end)) = start_tag(bytes, i) else { break };
        add_text(&stack, &mut elements, &bytes[text_from..i]);
        match name.as_str() {
            "head" if head.is_none() => { head = Some(end); in_head = true; }
            "html" if root.is_none() => root = Some(end),
            "body" => in_head = false,
            // Anything that cannot sit in head starts the body, as a browser reads it.
            tag if in_head && !HEAD_CONTENT.contains(&tag) => in_head = false,
            _ => {}
        }
        if let Some((_, closes)) = IMPLIED_END.iter().find(|(tag, _)| *tag == name) {
            if stack.last().is_some_and(|&e| closes.contains(&elements[e].tag.as_str())) { stack.pop(); }
        }
        let mut sorted: Vec<(String, String)> = Vec::new();
        let mut has_id = false;
        for (attr, value) in attributes {
            if attr == ATTRIBUTE {
                has_id = true;
                taken.insert(value);
                continue;
            }
            if attr == "id" && !value.trim().is_empty() { has_id = true; }
            sorted.push((attr, value));
        }
        sorted.sort();
        let tag_end = i + 1 + name.len();
        elements.push(Element {
            stamp: !has_id && !in_head && !UNSTAMPED.contains(&name.as_str()),
            attributes: sorted.iter().map(|(a, v)| format!("{a}={v}")).collect::<Vec<_>>().join("\u{1}"),
            insert_at: tag_end,
            text: String::new(),
            tag: name.clone(),
        });
        i = end;
        text_from = i;
        if RAW_TEXT.contains(&name.as_str()) {
            // Its contents are never read as markup, nor changed.
            i = match name.as_str() {
                "plaintext" => bytes.len(),
                "script" => script_end(bytes, i).unwrap_or(bytes.len()),
                _ => raw_text_end(bytes, i, &name).unwrap_or(bytes.len()),
            };
            text_from = i;
        } else if !self_closing && !VOID.contains(&name.as_str()) {
            stack.push(elements.len() - 1);
        }
    }
    add_text(&stack, &mut elements, &bytes[text_from.min(bytes.len())..]);
    Reading { elements, taken, early: head.or(root).or(doctype).unwrap_or(0), body_end }
}

/// FNV-1a, 64 bits: stable across builds and platforms, unlike `DefaultHasher`.
fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// Every id the document will carry, by insertion offset.
fn ids(reading: &Reading) -> Vec<(usize, String)> {
    let mut seen: HashMap<String, u32> = HashMap::new();
    let mut used = reading.taken.clone();
    let mut out = Vec::new();
    for element in reading.elements.iter().filter(|e| e.stamp) {
        let hash = fnv(&format!("{}\u{0}{}\u{0}{}", element.tag, element.attributes, element.text));
        let base = format!("n{:08x}", (hash ^ (hash >> 32)) as u32);
        let count = seen.entry(base.clone()).or_insert(0);
        let mut id;
        loop {
            *count += 1;
            id = if *count == 1 { base.clone() } else { format!("{base}-{count}") };
            if !used.contains(&id) { break; }
        }
        used.insert(id.clone());
        out.push((element.insert_at, id));
    }
    out
}

/// `html` with ids stamped, `early` inserted after the first `<head>` (or
/// `<html>`, or the doctype, or at the very start) and `late` before the last
/// `</body>` (or at the end).
pub fn prepare(html: &str, early: &str, late: &str) -> String {
    let reading = read(html);
    let mut inserts: Vec<(usize, String)> = ids(&reading).into_iter().map(|(at, id)| (at, format!(" {ATTRIBUTE}=\"{id}\""))).collect();
    if !early.is_empty() { inserts.push((reading.early, early.to_string())); }
    if !late.is_empty() { inserts.push((reading.body_end.unwrap_or(html.len()), late.to_string())); }
    inserts.sort_by_key(|(at, _)| *at);
    let mut out = String::with_capacity(html.len() + inserts.iter().map(|(_, s)| s.len()).sum::<usize>());
    let mut from = 0;
    for (at, text) in inserts {
        out.push_str(&html[from..at]);
        out.push_str(&text);
        from = at;
    }
    out.push_str(&html[from..]);
    out
}

/// `html` with ids stamped and nothing else.
pub fn stamp(html: &str) -> String {
    prepare(html, "", "")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `data-neko-id` of the first element whose opening tag contains `marker`.
    fn id_of(html: &str, marker: &str) -> Option<String> {
        let at = html.find(marker)?;
        let open = html[..at + 1].rfind('<')?;
        let close = open + html[open..].find('>')?;
        let tag = &html[open..close];
        let start = tag.find("data-neko-id=\"")? + 14;
        Some(tag[start..start + tag[start..].find('"')?].to_string())
    }

    /// Takes the stamps back out.
    fn unstamped(html: &str) -> String {
        let mut out = html.to_string();
        while let Some(at) = out.find(" data-neko-id=\"n") {
            let end = at + 15 + out[at + 15..].find('"').unwrap() + 1;
            out.replace_range(at..end, "");
        }
        out
    }

    #[test]
    fn every_drawn_element_without_an_id_gets_one_and_nothing_else_changes() {
        let html = "<!doctype html><html><head><meta charset=utf-8><title>A <b> title</title><style>p > b { color: red }</style></head>\n<body><main class=\"x\"><h1 id=\"top\">Hi</h1><p>One <b>two</b></p><img src=a.png><br></main></body></html>";
        let out = stamp(html);
        assert_eq!(unstamped(&out), html, "only the attribute is added");
        assert!(id_of(&out, "class=\"x\"").is_some(), "main");
        assert!(id_of(&out, "<p").is_some());
        assert!(out.contains("<title>A <b> title</title>"), "the <b> inside <title> is text");
        assert!(out.contains("<b data-neko-id=\"n"), "the <b> in the paragraph is stamped");
        assert!(out.contains("<img data-neko-id=\"n"), "void elements that are drawn are stamped");
        assert!(out.contains("<h1 id=\"top\">"), "an element with an id keeps it and gets none");
        for frame in ["<html>", "<head>", "<body>", "<meta charset=utf-8>", "<br>", "<title>", "<style>"] {
            assert!(out.contains(frame), "{frame} is not stamped");
        }
        assert_eq!(out.matches('\n').count(), html.matches('\n').count(), "no new lines");
    }

    #[test]
    fn scripts_styles_comments_and_quoted_values_are_never_touched() {
        let html = concat!(
            "<body><script>const t = '<div class=\"a\">'; if (a < b && c > d) document.write(\"<p>x</p>\");</script>",
            "<SCRIPT type=module>let s = `</scrip` + `t><span>`;</SCRIPT >",
            "<style>div::after { content: \"<em>\" }</style>",
            "<!-- <div>hidden</div> --><textarea><div>typed</div></textarea>",
            "<a title=\"1 > 0 <i>\" href=/x/y>link</a><svg><![CDATA[ <g> ]]><rect/></svg>",
            "<div data-neko-id=\"kept\">mine</div></body>",
        );
        let out = stamp(html);
        assert_eq!(unstamped(&out), html);
        for kept in [
            "<script>const t = '<div class=\"a\">'; if (a < b && c > d) document.write(\"<p>x</p>\");</script>",
            "<SCRIPT type=module>let s = `</scrip` + `t><span>`;</SCRIPT >",
            "<style>div::after { content: \"<em>\" }</style>",
            "<!-- <div>hidden</div> -->",
            "><div>typed</div></textarea>",
            "<![CDATA[ <g> ]]>",
            "<div data-neko-id=\"kept\">mine</div>",
        ] {
            assert!(out.contains(kept), "{kept}");
        }
        assert!(out.contains("<a data-neko-id=\"n") && out.contains("title=\"1 > 0 <i>\" href=/x/y>link</a>"));
        assert!(out.contains("<svg data-neko-id=\"n") && out.contains("<rect data-neko-id=\"n"));
        assert!(!out.contains("<i data-neko-id") && !out.contains("<span data-neko-id") && !out.contains("<em data-neko-id"));
    }

    #[test]
    fn ids_are_deterministic_unique_and_survive_a_new_sibling() {
        let v1 = "<ul><li class=item>Alpha</li><li class=item>Beta</li><li>Same</li><li>Same</li></ul>";
        assert_eq!(stamp(v1), stamp(v1), "the same document stamps the same");
        let out = stamp(v1);
        let ids: Vec<&str> = out.match_indices("data-neko-id=\"").map(|(at, _)| { let s = at + 14; &out[s..s + out[s..].find('"').unwrap()] }).collect();
        // ul, Alpha, Beta, Same, Same.
        assert_eq!(ids.len(), 5);
        assert_eq!(ids.iter().collect::<HashSet<_>>().len(), 5, "unique: {ids:?}");
        assert_eq!(ids[4], format!("{}-2", ids[3]), "identical elements are told apart by order");
        // v2 adds an item first: the unchanged items keep their ids; the list itself changed.
        let v2 = "<ul><li class=item>New</li><li class=item>Alpha</li><li class=item>Beta</li><li>Same</li><li>Same</li></ul>";
        let next = stamp(v2);
        assert_eq!(id_of(&next, ">Alpha").as_deref(), Some(ids[1]), "Alpha keeps its id");
        assert_eq!(id_of(&next, ">Beta").as_deref(), Some(ids[2]), "Beta keeps its id");
        assert_eq!(id_of(&next, ">Same").as_deref(), Some(ids[3]));
        assert_ne!(id_of(&next, "<ul").as_deref(), Some(ids[0]), "the list's text changed, so its id did");
        // A changed element gets a new id.
        let v3 = v1.replace("Alpha", "Alpha!");
        assert!(!stamp(&v3).contains(&format!("data-neko-id=\"{}\"", ids[1])));
        // An id an earlier version carried and the agent kept is never handed out twice.
        let kept = format!("<p data-neko-id=\"{}\">x</p><ul><li class=item>Alpha</li></ul>", ids[1]);
        assert_eq!(stamp(&kept).matches(&format!("\"{}\"", ids[1])).count(), 1);
    }

    #[test]
    fn comments_cdata_and_scripts_end_where_a_browser_ends_them() {
        for (html, inside) in [
            // `<!-->` and `<!--->` are whole comments, so the script after them is a script.
            ("<!--> <script>var s=\"--> <div>\";</script><p>a</p>", "<script>var s=\"--> <div>\";</script>"),
            ("<!---> <script>var s=\"<div>\";</script><p>a</p>", "<script>var s=\"<div>\";</script>"),
            // `--!>` ends a comment.
            ("<!-- x --!><script>let a = '<b>';</script><p>a</p>", "<script>let a = '<b>';</script>"),
            // Outside svg and math, `<![CDATA[` is a bogus comment that ends at the first `>`.
            ("<![CDATA[ x ]><script>let t = '<i>';</script><p>a</p>", "<script>let t = '<i>';</script>"),
            // Script data escaped states: a nested `<script>` after `<!--` keeps `</script>` inside.
            ("<script><!-- document.write('<script>x()</script><div>'); --></script><p>a</p>", "<script><!-- document.write('<script>x()</script><div>'); --></script>"),
            ("<script><!--></script><p>a</p>", "<script><!--></script>"),
        ] {
            let out = stamp(html);
            assert!(out.contains(inside), "{html} -> {out}");
            assert_eq!(unstamped(&out), html);
            assert!(out.contains("<p data-neko-id=\"n"), "what follows is still read: {out}");
        }
        // Inside svg, CDATA is CDATA.
        let svg = stamp("<svg><![CDATA[ <g> > ]]><rect/></svg>");
        assert!(svg.contains("<![CDATA[ <g> > ]]>") && svg.contains("<rect data-neko-id"));
    }

    #[test]
    fn a_head_left_open_ends_at_the_first_element_that_cannot_be_in_it() {
        let out = stamp("<html><head><title>t</title><meta charset=utf-8><div>body text</div><p>more</p>");
        assert!(out.contains("<div data-neko-id=\"n") && out.contains("<p data-neko-id=\"n"), "{out}");
        assert!(out.contains("<meta charset=utf-8>"));
    }

    #[test]
    fn unclosed_list_items_and_paragraphs_close_each_other() {
        // Without the implied end, the first <li> would hold the second's text.
        let a = stamp("<ul><li>One<li>Two</ul>");
        let b = stamp("<ul><li>One<li>Three</ul>");
        let first = |html: &str| id_of(html, "<li").unwrap();
        assert_eq!(first(&a), first(&b));
    }

    #[test]
    fn the_bridges_go_where_they_run_first_and_last() {
        let early = "<script data-neko-bridge>E</script>";
        let late = "<script data-neko-bridge>L</script>";
        let page = prepare("<!DOCTYPE html>\n<html lang=en><head><title>x</title></head><body><p>a</p></BODY>\n</html>", early, late);
        assert!(page.starts_with("<!DOCTYPE html>\n<html lang=en><head><script data-neko-bridge>E</script><title>"), "{page}");
        assert!(page.ends_with("<script data-neko-bridge>L</script></BODY>\n</html>"), "{page}");
        assert!(prepare("<!doctype html><p>x</p>", early, late).starts_with("<!doctype html><script data-neko-bridge>E</script><p"));
        assert!(prepare("<html><p>x</p>", early, late).starts_with("<html><script data-neko-bridge>E"));
        let bare = prepare("<p>x</p>", early, late);
        assert!(bare.starts_with(early) && bare.ends_with(late));
        // A `</body>` inside a script is not the body's end.
        let tricky = prepare("<body><script>let s='</body>';</script></body>", "", late);
        assert!(tricky.ends_with("</script><script data-neko-bridge>L</script></body>"), "{tricky}");
    }

    #[test]
    fn broken_markup_is_survived_not_rewritten() {
        for html in ["<div class=\"never closed", "<", "</", "<!--", "<script>never ends", "<p>ok<", "<a b='x>", "<ü>text</ü>", "<<p>>"] {
            let out = stamp(html);
            assert_eq!(unstamped(&out), html, "{html}");
        }
        // Big input stays linear enough to serve.
        let big = "<div><p>word word word</p></div>".repeat(20_000);
        let out = stamp(&big);
        assert_eq!(out.matches("data-neko-id").count(), 40_000);
    }
}
