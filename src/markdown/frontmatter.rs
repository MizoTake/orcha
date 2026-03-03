use serde::de::DeserializeOwned;
use serde::Serialize;

/// A markdown document with YAML frontmatter.
#[derive(Debug, Clone)]
pub struct Document<T> {
    pub frontmatter: T,
    pub content: String,
}

/// Parse a markdown file with YAML frontmatter delimited by `---`.
pub fn parse<T: DeserializeOwned>(input: &str) -> anyhow::Result<Document<T>> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with("---") {
        anyhow::bail!("Missing YAML frontmatter delimiter '---'");
    }

    let after_first = &trimmed[3..];
    let end_pos = after_first
        .find("\n---")
        .ok_or_else(|| anyhow::anyhow!("Missing closing frontmatter delimiter '---'"))?;

    let yaml_str = &after_first[..end_pos];
    let content_start = end_pos + 4; // skip "\n---"
    let content = if content_start < after_first.len() {
        after_first[content_start..]
            .trim_start_matches('\n')
            .to_string()
    } else {
        String::new()
    };

    let frontmatter: T = serde_yaml::from_str(yaml_str)?;

    Ok(Document {
        frontmatter,
        content,
    })
}

/// Serialize a document back to markdown with YAML frontmatter.
pub fn serialize<T: Serialize>(doc: &Document<T>) -> anyhow::Result<String> {
    let yaml = serde_yaml::to_string(&doc.frontmatter)?;
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&yaml);
    out.push_str("---\n");
    if !doc.content.is_empty() {
        out.push('\n');
        out.push_str(&doc.content);
        if !doc.content.ends_with('\n') {
            out.push('\n');
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestFrontmatter {
        title: String,
        count: u32,
    }

    #[test]
    fn parse_basic_frontmatter() {
        let input = "---\ntitle: hello\ncount: 42\n---\n# Body\n\nSome content.\n";
        let doc: Document<TestFrontmatter> = parse(input).unwrap();
        assert_eq!(doc.frontmatter.title, "hello");
        assert_eq!(doc.frontmatter.count, 42);
        assert!(doc.content.contains("# Body"));
        assert!(doc.content.contains("Some content."));
    }

    #[test]
    fn roundtrip() {
        let doc = Document {
            frontmatter: TestFrontmatter {
                title: "test".into(),
                count: 10,
            },
            content: "# Hello\n\nWorld.\n".into(),
        };
        let serialized = serialize(&doc).unwrap();
        let parsed: Document<TestFrontmatter> = parse(&serialized).unwrap();
        assert_eq!(parsed.frontmatter, doc.frontmatter);
        assert!(parsed.content.contains("# Hello"));
    }

    #[test]
    fn missing_frontmatter_errors() {
        let input = "# No frontmatter here\n";
        assert!(parse::<TestFrontmatter>(input).is_err());
    }

    #[test]
    fn missing_closing_delimiter_errors() {
        let input = "---\ntitle: hello\ncount: 1\n# no closing delimiter";
        assert!(parse::<TestFrontmatter>(input).is_err());
    }

    #[test]
    fn dashes_in_body_do_not_confuse_parser() {
        // The body contains a line with "---" but it is NOT the closing delimiter
        // because the parser looks for "\n---" after the yaml region.
        let input = "---\ntitle: hello\ncount: 42\n---\n# Body\n\n---\n\nSome text after hr.\n";
        let doc: Document<TestFrontmatter> = parse(input).unwrap();
        assert_eq!(doc.frontmatter.title, "hello");
        // The HR and subsequent text should appear in the content.
        assert!(doc.content.contains("---"));
        assert!(doc.content.contains("Some text after hr."));
    }

    #[test]
    fn empty_body_produces_empty_content_string() {
        let input = "---\ntitle: hello\ncount: 0\n---\n";
        let doc: Document<TestFrontmatter> = parse(input).unwrap();
        assert!(doc.content.is_empty());
    }

    #[test]
    fn serialize_empty_content_omits_body_newline() {
        let doc = Document {
            frontmatter: TestFrontmatter {
                title: "t".into(),
                count: 0,
            },
            content: String::new(),
        };
        let out = serialize(&doc).unwrap();
        // Should end exactly at "---\n" with no trailing newline after it.
        assert!(out.ends_with("---\n"));
        assert!(!out.contains("\n\n"));
    }

    #[test]
    fn serialize_adds_trailing_newline_if_content_lacks_one() {
        let doc = Document {
            frontmatter: TestFrontmatter {
                title: "t".into(),
                count: 0,
            },
            content: "# Hello".into(), // no trailing newline
        };
        let out = serialize(&doc).unwrap();
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn leading_whitespace_before_delimiter_is_tolerated() {
        let input = "  \n---\ntitle: hello\ncount: 1\n---\n";
        let doc: Document<TestFrontmatter> = parse(input).unwrap();
        assert_eq!(doc.frontmatter.title, "hello");
    }
}
