use super::*;

// ── core behaviour (what OUR code does) ─────────────────────────────

#[test]
fn strips_script_and_style_subtrees() {
    // Our collect_text() must skip <script> and <style> subtrees entirely.
    let html = "<html><script>var x = 1; alert('hi')</script><p>keep</p></html>";
    let result = strip_html(html);
    assert!(result.contains("keep"));
    assert!(!result.contains("var"));
    assert!(!result.contains("alert"));

    let html = "<html><style>.a{color:red}</style><p>keep</p></html>";
    let result = strip_html(html);
    assert!(result.contains("keep"));
    assert!(!result.contains("color"));
}

#[test]
fn strips_plain_tags() {
    // Plain tags are stripped; text inside them is kept.
    let result = strip_html("<p>Hello <b>World</b></p>");
    assert!(result.contains("Hello"));
    assert!(result.contains("World"));
}

#[test]
fn normalizes_whitespace() {
    // Runs of whitespace (spaces, newlines, tabs) collapse to a single space.
    let result = strip_html("<p>a   b\n\nc\t\td</p>");
    assert_eq!(result, "a b c d");
}

#[test]
fn empty_input() {
    assert_eq!(strip_html(""), "");
}

#[test]
fn only_whitespace() {
    assert_eq!(strip_html("   \n\t  "), "");
}

// ── contract: behaviour the caller depends on ────────────────────────

#[test]
fn entities_decoded() {
    // html5ever decodes entities during parsing.  The caller (WebFetch
    // tool) expects readable text, not raw HTML entities.
    let result = strip_html("a&amp;b &lt; c &gt; d");
    assert_eq!(result, "a&b < c > d");
}

#[test]
fn case_insensitive_tags() {
    // HTML5 parser is case-insensitive for tag names.
    let result = strip_html("<SCRIPT>alert(1)</script><p>keep</p>");
    assert!(result.contains("keep"));
    assert!(!result.contains("alert"));
}

#[test]
fn chinese_text_preserved() {
    // Multibyte UTF-8 content must survive round-trip intact.
    let result = strip_html("<p>你好世界</p>");
    assert_eq!(result, "你好世界");
}

#[test]
fn chinese_inside_script_stripped() {
    // Chinese inside a script block must be removed.
    let html = "<script>var x = '中文内容'; x > 5</script><p>visible</p>";
    let result = strip_html(html);
    assert_eq!(result.trim(), "visible");
}

#[test]
fn fullwidth_bracket_in_tag() {
    // Non-ASCII characters in tag names — treated as part of the tag,
    // not as content.  Text inside is preserved.
    let html = "<meta【charset=\"utf-8\">content</meta【><p>visible</p>";
    let result = strip_html(html);
    assert!(result.contains("visible"));
    assert!(!result.contains("charset"));
}
