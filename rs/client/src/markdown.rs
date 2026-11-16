//! Markdown to blocks parser for UI rendering.

use serde::{Deserialize, Serialize};

/// A block of content (heading, paragraph, list, code, table, etc).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Block {
    /// A heading (level 1-6).
    Heading { level: u8, text: String },
    /// A paragraph of text.
    Paragraph { text: String },
    /// A code block.
    Code { lang: String, text: String },
    /// A list of items.
    List { ordered: bool, items: Vec<String> },
    /// A table with headers and rows.
    Table { headers: Vec<String>, rows: Vec<Vec<String>> },
    /// A blockquote.
    Quote { text: String },
    /// A horizontal rule.
    Rule,
}

/// Parse markdown into blocks.
pub fn parse(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Skip empty lines
        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        // Headings
        if let Some(rest) = line.strip_prefix('#') {
            let level = line.len() - rest.len();
            if level <= 6 && rest.starts_with(' ') {
                blocks.push(Block::Heading {
                    level: level as u8,
                    text: rest[1..].trim().to_string(),
                });
                i += 1;
                continue;
            }
        }

        // Code blocks (triple backtick)
        if line.trim_start().starts_with("```") {
            let lang = line
                .trim_start()
                .strip_prefix("```")
                .unwrap_or("")
                .trim()
                .to_string();
            let mut code_lines = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code_lines.push(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                i += 1; // skip closing ```
            }
            blocks.push(Block::Code {
                lang,
                text: code_lines.join("\n"),
            });
            continue;
        }

        // Blockquotes
        if line.trim_start().starts_with('>') {
            let mut quote_lines = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                let content = lines[i]
                    .trim_start()
                    .strip_prefix(">")
                    .unwrap_or("")
                    .trim();
                quote_lines.push(content);
                i += 1;
            }
            blocks.push(Block::Quote {
                text: quote_lines.join("\n"),
            });
            continue;
        }

        // Horizontal rule
        if line.trim() == "---" || line.trim() == "***" || line.trim() == "___" {
            blocks.push(Block::Rule);
            i += 1;
            continue;
        }

        // Lists (unordered)
        if line.trim_start().starts_with("- ") || line.trim_start().starts_with("* ") {
            let mut items = Vec::new();
            while i < lines.len() {
                let l = lines[i];
                if let Some(rest) = l.trim_start().strip_prefix("- ") {
                    items.push(rest.to_string());
                    i += 1;
                } else if let Some(rest) = l.trim_start().strip_prefix("* ") {
                    items.push(rest.to_string());
                    i += 1;
                } else if l.trim().is_empty() {
                    i += 1;
                } else {
                    break;
                }
            }
            blocks.push(Block::List {
                ordered: false,
                items,
            });
            continue;
        }

        // Lists (ordered)
        if line.trim_start().chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            if let Some(_rest) = line.trim_start().split_once(". ") {
                let mut items = Vec::new();
                while i < lines.len() {
                    let l = lines[i];
                    if l.trim_start().chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                        if let Some((_, rest)) = l.trim_start().split_once(". ") {
                            items.push(rest.to_string());
                            i += 1;
                        } else {
                            break;
                        }
                    } else if l.trim().is_empty() {
                        i += 1;
                    } else {
                        break;
                    }
                }
                if !items.is_empty() {
                    blocks.push(Block::List {
                        ordered: true,
                        items,
                    });
                    continue;
                }
            }
        }

        // Tables (pipe-separated)
        if line.contains('|') && i + 1 < lines.len() && lines[i + 1].contains('|') {
            let next = lines[i + 1];
            // Check if next line is a separator (hyphens and pipes)
            if next
                .split('|')
                .map(|s| s.trim())
                .all(|s| s.is_empty() || s.chars().all(|c| c == '-'))
            {
                let headers: Vec<String> = line
                    .split('|')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                let mut rows = Vec::new();
                i += 2;
                while i < lines.len() && lines[i].contains('|') {
                    let row: Vec<String> = lines[i]
                        .split('|')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !row.is_empty() {
                        rows.push(row);
                    }
                    i += 1;
                }
                if !headers.is_empty() {
                    blocks.push(Block::Table { headers, rows });
                    continue;
                }
            }
        }

        // Default: paragraph
        let mut para_lines = vec![line];
        i += 1;
        while i < lines.len() {
            let l = lines[i];
            if l.trim().is_empty()
                || l.trim_start().starts_with('#')
                || l.trim_start().starts_with("```")
                || l.trim_start().starts_with('>')
            {
                break;
            }
            para_lines.push(l);
            i += 1;
        }
        blocks.push(Block::Paragraph {
            text: para_lines.join(" "),
        });
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heading() {
        let blocks = parse("# Title\nSome text");
        assert_eq!(blocks.len(), 2);
        assert!(matches!(blocks[0], Block::Heading { level: 1, .. }));
    }

    #[test]
    fn test_code_block() {
        let blocks = parse("```rust\nfn main() {}\n```");
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], Block::Code { lang, .. } if lang == "rust"));
    }

    #[test]
    fn test_list() {
        let blocks = parse("- Item 1\n- Item 2");
        assert_eq!(blocks.len(), 1);
        assert!(matches!(&blocks[0], Block::List { ordered: false, items } if items.len() == 2));
    }

    #[test]
    fn test_table() {
        let blocks = parse("| A | B |\n|---|---|\n| 1 | 2 |");
        assert_eq!(blocks.len(), 1);
        assert!(matches!(blocks[0], Block::Table { .. }));
    }

    #[test]
    fn test_blockquote() {
        let blocks = parse("> A quote\n> More quote");
        assert_eq!(blocks.len(), 1);
        assert!(matches!(blocks[0], Block::Quote { .. }));
    }

    #[test]
    fn test_horizontal_rule() {
        let blocks = parse("---");
        assert_eq!(blocks.len(), 1);
        assert!(matches!(blocks[0], Block::Rule));
    }

    #[test]
    fn test_paragraph() {
        let blocks = parse("Just some text");
        assert_eq!(blocks.len(), 1);
        assert!(matches!(blocks[0], Block::Paragraph { .. }));
    }
}
