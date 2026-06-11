use super::*;

#[test]
fn strip_basic_html() {
    let result = strip_html("<p>Hello <b>World</b></p>");
    assert!(result.contains("Hello"));
    assert!(result.contains("World"));
}

#[test]
fn strip_script_tags() {
    let result = strip_html("<html><script>alert('hi')</script><body>content</body></html>");
    assert!(result.contains("content"));
    assert!(!result.contains("alert"));
    assert!(!result.contains("script"));
}

#[test]
fn strip_style_tags() {
    let result = strip_html("<style>body{color:red}</style><p>text</p>");
    assert!(result.contains("text"));
    assert!(!result.contains("color"));
}

#[test]
fn decode_entities() {
    let result = strip_html("a&amp;b &lt; c &gt; d");
    assert!(result.contains("a&b"));
    assert!(result.contains("< c"));
    assert!(result.contains("> d"));
}

#[test]
fn empty_html() {
    let result = strip_html("");
    assert!(result.is_empty());
}
