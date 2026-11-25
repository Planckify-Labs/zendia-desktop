/*
 * Copyright (c) 2026-present, the Zendia developers.
 *
 * SPDX-License-Identifier: BSD-2-Clause
 */

//! Streaming-safe GitHub-Flavored Markdown parser for agent responses.
//!
//! Produces a pure AST (blocks and inline spans) with no HTML output so views can render natively with theme roles.
//! When `partial` is true, unclosed code fences, inline code spans, and emphasis markers at the end of the text are
//! rendered as if closed so streaming answers never flash raw markup.

/// Horizontal alignment of a Markdown table column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColumnAlign {
    #[default]
    None,
    Left,
    Center,
    Right,
}

/// One inline element inside a paragraph, heading, list item, or table cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MdInline {
    Text(String),
    Break,
    Code(String),
    Strong(Vec<MdInline>),
    Em(Vec<MdInline>),
    Del(Vec<MdInline>),
    Link { href: String, children: Vec<MdInline> },
}

/// One block-level Markdown element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MdBlock {
    Heading {
        level: u8,
        inlines: Vec<MdInline>,
    },
    Paragraph {
        inlines: Vec<MdInline>,
    },
    Code {
        lang: Option<String>,
        text: String,
        open: bool,
    },
    Quote {
        blocks: Vec<MdBlock>,
    },
    List {
        ordered: bool,
        start: u64,
        items: Vec<Vec<MdBlock>>,
    },
    Rule,
    Table {
        header: Vec<Vec<MdInline>>,
        align: Vec<ColumnAlign>,
        rows: Vec<Vec<Vec<MdInline>>>,
    },
}

/// Parses `text` into a list of [`MdBlock`] values.
///
/// Set `partial = true` while an agent response is actively streaming so incomplete trailing markup stays formatted.
pub fn parse_markdown(text: &str, partial: bool) -> Vec<MdBlock> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    parse_block_lines(&lines, partial)
}

/// Parses inline Markdown syntax within a single block's text.
pub fn parse_inline(text: &str, partial: bool) -> Vec<MdInline> {
    let mut nodes = Vec::new();
    let mut plain = String::new();
    let bytes = text.as_bytes();
    let len = text.len();
    let mut i = 0;

    while i < len {
        let rest = &text[i..];
        let ch = rest.chars().next().unwrap();
        let ch_len = ch.len_utf8();

        if ch == '\\'
            && i + 1 < len
            && let Some(next_ch) = text[i + 1..].chars().next()
            && is_escapable(next_ch)
        {
            plain.push(next_ch);
            i += 1 + next_ch.len_utf8();
            continue;
        }

        if ch == '\n' {
            flush_plain(&mut plain, &mut nodes);
            nodes.push(MdInline::Break);
            i += 1;
            continue;
        }

        let parsed = match ch {
            '`' => read_code(text, i, partial),
            '[' => read_link(text, i),
            'h' => read_autolink(text, i),
            '*' | '_' | '~' => read_emphasis(text, i, partial),
            _ => None,
        };

        if let Some((inline, consumed)) = parsed {
            match inline {
                InlineRead::Node(node) => {
                    flush_plain(&mut plain, &mut nodes);
                    nodes.push(node);
                }
                InlineRead::Literal(lit) => {
                    plain.push_str(lit);
                }
            }
            i += consumed.max(1);
        } else {
            plain.push(ch);
            i += ch_len;
        }
        let _ = bytes;
    }

    flush_plain(&mut plain, &mut nodes);
    nodes
}

/// Returns `Some(href)` only for safe `http:`, `https:`, and `mailto:` URLs.
pub fn safe_href(href: &str) -> Option<String> {
    let trimmed = href.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:") {
        Some(trimmed.to_string())
    } else {
        None
    }
}

enum InlineRead<'a> {
    Node(MdInline),
    Literal(&'a str),
}

fn flush_plain(plain: &mut String, nodes: &mut Vec<MdInline>) {
    if !plain.is_empty() {
        nodes.push(MdInline::Text(std::mem::take(plain)));
    }
}

fn is_escapable(ch: char) -> bool {
    matches!(
        ch,
        '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '-' | '.' | '!' | '|' | '~' | '>'
    )
}

fn indent_of(line: &str) -> usize {
    let mut cols = 0;
    for ch in line.chars() {
        match ch {
            ' ' => cols += 1,
            '\t' => cols += 4,
            _ => break,
        }
    }
    cols
}

fn dedent(line: &str, count: usize) -> &str {
    let mut removed = 0;
    let mut byte_idx = 0;
    for ch in line.chars() {
        if removed >= count {
            break;
        }
        match ch {
            ' ' => {
                removed += 1;
                byte_idx += 1;
            }
            '\t' => {
                removed += 4;
                byte_idx += 1;
            }
            _ => break,
        }
    }
    &line[byte_idx..]
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// Leading spaces up to `max_spaces`, returning the slice after them.
fn strip_leading_spaces(line: &str, max_spaces: usize) -> Option<&str> {
    let mut count = 0;
    let bytes = line.as_bytes();
    while count < bytes.len() && bytes[count] == b' ' {
        count += 1;
    }
    if count <= max_spaces {
        Some(&line[count..])
    } else {
        None
    }
}

/// Parses a fenced code opening line (` ```rust ` or `~~~`), returning `(fence_char, fence_len, lang)`.
fn match_fence(line: &str) -> Option<(char, usize, Option<String>)> {
    let rest = strip_leading_spaces(line, 3)?;
    let first = rest.chars().next()?;
    if first != '`' && first != '~' {
        return None;
    }
    let fence_len = rest.chars().take_while(|&c| c == first).count();
    if fence_len < 3 {
        return None;
    }
    let after = rest[fence_len..].trim();
    if first == '`' && after.contains('`') {
        return None;
    }
    let lang = after
        .split_whitespace()
        .next()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Some((first, fence_len, lang))
}

fn is_fence_close(line: &str, fence_char: char, min_len: usize) -> bool {
    let trimmed = line.trim();
    let count = trimmed.chars().take_while(|&c| c == fence_char).count();
    count >= min_len && count == trimmed.len()
}

fn is_rule(line: &str) -> bool {
    let Some(rest) = strip_leading_spaces(line, 3) else {
        return false;
    };
    let trimmed = rest.trim_end();
    let Some(marker) = trimmed.chars().next() else {
        return false;
    };
    if !matches!(marker, '-' | '*' | '_') {
        return false;
    }
    let mut count = 0;
    for ch in trimmed.chars() {
        if ch == marker {
            count += 1;
        } else if ch != ' ' && ch != '\t' {
            return false;
        }
    }
    count >= 3
}

fn match_heading(line: &str) -> Option<(u8, String)> {
    let rest = strip_leading_spaces(line, 3)?;
    let hashes = rest.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let after = &rest[hashes..];
    if !after.is_empty() && !after.starts_with(' ') && !after.starts_with('\t') {
        return None;
    }
    let mut body = after.trim();
    // Optional closing hashes preceded by whitespace.
    if let Some(without_trailing) = body.strip_suffix('#') {
        let stripped = without_trailing.trim_end_matches('#');
        if stripped.is_empty() || stripped.ends_with(' ') || stripped.ends_with('\t') {
            body = stripped.trim_end();
        }
    }
    Some((hashes as u8, body.to_string()))
}

fn strip_quote(line: &str) -> Option<&str> {
    let rest = strip_leading_spaces(line, 3)?;
    let after = rest.strip_prefix('>')?;
    Some(
        after
            .strip_prefix(' ')
            .or_else(|| after.strip_prefix('\t'))
            .unwrap_or(after),
    )
}

struct ListItemMatch<'a> {
    indent: usize,
    ordered: bool,
    number: u64,
    marker_len: usize,
    gap_len: usize,
    rest: &'a str,
}

fn match_list_item(line: &str) -> Option<ListItemMatch<'_>> {
    let indent = line.bytes().take_while(|&b| b == b' ').count();
    if indent > 12 {
        return None;
    }
    let rest = &line[indent..];
    let bytes = rest.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    if matches!(bytes[0], b'-' | b'*' | b'+') {
        let after = &rest[1..];
        let gap = after.bytes().take_while(|&b| b == b' ' || b == b'\t').count();
        if gap == 0 && !after.is_empty() {
            return None;
        }
        return Some(ListItemMatch {
            indent,
            ordered: false,
            number: 1,
            marker_len: 1,
            gap_len: gap,
            rest: &after[gap..],
        });
    }

    let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if !(1..=9).contains(&digits) || digits >= bytes.len() {
        return None;
    }
    if !matches!(bytes[digits], b'.' | b')') {
        return None;
    }
    let marker_len = digits + 1;
    let after = &rest[marker_len..];
    let gap = after.bytes().take_while(|&b| b == b' ' || b == b'\t').count();
    if gap == 0 && !after.is_empty() {
        return None;
    }
    let number = rest[..digits].parse::<u64>().ok()?;
    Some(ListItemMatch {
        indent,
        ordered: true,
        number,
        marker_len,
        gap_len: gap,
        rest: &after[gap..],
    })
}

fn is_table_delimiter(line: &str) -> bool {
    let Some(rest) = strip_leading_spaces(line, 3) else {
        return false;
    };
    let mut trimmed = rest.trim();
    if trimmed.is_empty() || !trimmed.contains('-') {
        return false;
    }
    if let Some(s) = trimmed.strip_prefix('|') {
        trimmed = s;
    }
    if let Some(s) = trimmed.strip_suffix('|') {
        trimmed = s;
    }
    if trimmed.is_empty() {
        return false;
    }
    for cell in trimmed.split('|') {
        let c = cell.trim();
        let inner = c.strip_prefix(':').unwrap_or(c);
        let inner = inner.strip_suffix(':').unwrap_or(inner);
        if inner.is_empty() || !inner.bytes().all(|b| b == b'-') {
            return false;
        }
    }
    true
}

fn is_table_start(lines: &[&str], idx: usize) -> bool {
    idx + 1 < lines.len()
        && lines[idx].contains('|')
        && is_table_delimiter(lines[idx + 1])
        && (lines[idx + 1].contains('|') || lines[idx].trim_start().starts_with('|'))
}

fn starts_block(lines: &[&str], idx: usize) -> bool {
    let line = lines[idx];
    match_fence(line).is_some()
        || is_rule(line)
        || match_heading(line).is_some()
        || strip_quote(line).is_some()
        || match_list_item(line).is_some_and(|m| !is_blank(m.rest))
        || is_table_start(lines, idx)
}

fn split_table_row(line: &str) -> Vec<String> {
    let mut row = line.trim();
    if let Some(s) = row.strip_prefix('|') {
        row = s;
    }
    if row.ends_with('|') && !row.ends_with("\\|") {
        row = &row[..row.len() - 1];
    }
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut in_code = false;
    let mut chars = row.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'|') {
            cell.push('|');
            chars.next();
        } else if ch == '`' {
            in_code = !in_code;
            cell.push(ch);
        } else if ch == '|' && !in_code {
            cells.push(cell.trim().to_string());
            cell.clear();
        } else {
            cell.push(ch);
        }
    }
    cells.push(cell.trim().to_string());
    cells
}

fn parse_list(lines: &[&str], start: usize, partial: bool) -> (MdBlock, usize) {
    let first = match_list_item(lines[start]).expect("parse_list called on non-list line");
    let ordered = first.ordered;
    let start_num = if ordered { first.number } else { 1 };
    let base_indent = first.indent;
    let mut items: Vec<Vec<MdBlock>> = Vec::new();
    let mut i = start;

    while i < lines.len() {
        let Some(m) = match_list_item(lines[i]) else {
            break;
        };
        if m.ordered != ordered || m.indent > base_indent + 1 || m.indent + 1 < base_indent {
            break;
        }
        let content_indent = (m.indent + m.marker_len + m.gap_len.max(1)).min(base_indent + 2);
        let mut item_lines: Vec<String> = vec![m.rest.to_string()];
        i += 1;
        let mut blank_before = false;
        while i < lines.len() {
            let line = lines[i];
            if is_blank(line) {
                blank_before = true;
                item_lines.push(String::new());
                i += 1;
                continue;
            }
            if indent_of(line) >= content_indent {
                item_lines.push(dedent(line, content_indent).to_string());
                blank_before = false;
                i += 1;
                continue;
            }
            if !blank_before && !starts_block(lines, i) {
                item_lines.push(line.trim().to_string());
                i += 1;
                continue;
            }
            break;
        }
        while item_lines.last().is_some_and(|l| is_blank(l)) {
            item_lines.pop();
            blank_before = true;
        }
        let is_last = i >= lines.len();
        let borrowed: Vec<&str> = item_lines.iter().map(String::as_str).collect();
        items.push(parse_block_lines(&borrowed, partial && is_last));
        if blank_before && !(i < lines.len() && match_list_item(lines[i]).is_some()) {
            break;
        }
    }

    (
        MdBlock::List {
            ordered,
            start: start_num,
            items,
        },
        i,
    )
}

enum RawBlock {
    Heading {
        level: u8,
        text: String,
    },
    Paragraph(String),
    Code {
        lang: Option<String>,
        text: String,
        open: bool,
    },
    Quote(Vec<MdBlock>),
    List(MdBlock),
    Rule,
    Table {
        header: Vec<String>,
        align: Vec<ColumnAlign>,
        rows: Vec<Vec<String>>,
    },
}

fn parse_block_lines(lines: &[&str], partial: bool) -> Vec<MdBlock> {
    let mut raw_blocks = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        if is_blank(line) {
            i += 1;
            continue;
        }

        if let Some((fence_char, fence_len, lang)) = match_fence(line) {
            let indent = indent_of(line);
            i += 1;
            let mut body = Vec::new();
            let mut closed = false;
            while i < lines.len() {
                if is_fence_close(lines[i], fence_char, fence_len) {
                    closed = true;
                    i += 1;
                    break;
                }
                body.push(dedent(lines[i], indent));
                i += 1;
            }
            raw_blocks.push(RawBlock::Code {
                lang,
                text: body.join("\n"),
                open: !closed,
            });
            continue;
        }

        if is_rule(line) {
            raw_blocks.push(RawBlock::Rule);
            i += 1;
            continue;
        }

        if let Some((level, text)) = match_heading(line) {
            raw_blocks.push(RawBlock::Heading { level, text });
            i += 1;
            continue;
        }

        if strip_quote(line).is_some() {
            let mut quoted = Vec::new();
            while i < lines.len()
                && let Some(stripped) = strip_quote(lines[i])
            {
                quoted.push(stripped);
                i += 1;
            }
            let blocks = parse_block_lines(&quoted, partial && i >= lines.len());
            raw_blocks.push(RawBlock::Quote(blocks));
            continue;
        }

        if match_list_item(line).is_some_and(|m| !is_blank(m.rest)) {
            let (list, next) = parse_list(lines, i, partial);
            raw_blocks.push(RawBlock::List(list));
            i = next;
            continue;
        }

        if is_table_start(lines, i) {
            let header = split_table_row(line);
            let align = split_table_row(lines[i + 1])
                .into_iter()
                .map(|cell| {
                    let left = cell.starts_with(':');
                    let right = cell.ends_with(':');
                    match (left, right) {
                        (true, true) => ColumnAlign::Center,
                        (false, true) => ColumnAlign::Right,
                        (true, false) => ColumnAlign::Left,
                        (false, false) => ColumnAlign::None,
                    }
                })
                .collect();
            i += 2;
            let mut rows = Vec::new();
            while i < lines.len() && !is_blank(lines[i]) && lines[i].contains('|') {
                rows.push(split_table_row(lines[i]));
                i += 1;
            }
            raw_blocks.push(RawBlock::Table { header, align, rows });
            continue;
        }

        let mut para = vec![line.trim()];
        i += 1;
        while i < lines.len() && !is_blank(lines[i]) && !starts_block(lines, i) {
            para.push(lines[i].trim());
            i += 1;
        }
        raw_blocks.push(RawBlock::Paragraph(para.join("\n")));
    }

    let total = raw_blocks.len();
    raw_blocks
        .into_iter()
        .enumerate()
        .map(|(idx, block)| {
            let is_partial = partial && idx + 1 == total;
            match block {
                RawBlock::Heading { level, text } => MdBlock::Heading {
                    level,
                    inlines: parse_inline(&text, is_partial),
                },
                RawBlock::Paragraph(text) => MdBlock::Paragraph {
                    inlines: parse_inline(&text, is_partial),
                },
                RawBlock::Code { lang, text, open } => MdBlock::Code { lang, text, open },
                RawBlock::Quote(blocks) => MdBlock::Quote { blocks },
                RawBlock::List(list) => list,
                RawBlock::Rule => MdBlock::Rule,
                RawBlock::Table { header, align, rows } => {
                    let col_count = header.len();
                    let parsed_header = header.into_iter().map(|cell| parse_inline(&cell, is_partial)).collect();
                    let parsed_rows = rows
                        .into_iter()
                        .map(|row| {
                            (0..col_count)
                                .map(|col| {
                                    let text = row.get(col).map(String::as_str).unwrap_or("");
                                    parse_inline(text, is_partial)
                                })
                                .collect()
                        })
                        .collect();
                    MdBlock::Table {
                        header: parsed_header,
                        align,
                        rows: parsed_rows,
                    }
                }
            }
        })
        .collect()
}

fn read_code<'a>(text: &'a str, start: usize, partial: bool) -> Option<(InlineRead<'a>, usize)> {
    let rest = &text[start..];
    let ticks_len = rest.bytes().take_while(|&b| b == b'`').count();
    let ticks = &rest[..ticks_len];

    let mut search = start + ticks_len;
    let mut found = None;
    while search < text.len() {
        let Some(rel) = text[search..].find(ticks) else {
            break;
        };
        let pos = search + rel;
        let run_end = pos + text[pos..].bytes().take_while(|&b| b == b'`').count();
        if run_end - pos == ticks_len {
            found = Some(pos);
            break;
        }
        search = run_end;
    }

    if found.is_none() && !partial {
        return Some((InlineRead::Literal(ticks), ticks_len));
    }

    let raw = match found {
        Some(end) => &text[start + ticks_len..end],
        None => &text[start + ticks_len..],
    };
    let trimmed = if raw.len() >= 2 && raw.starts_with(' ') && raw.ends_with(' ') && raw.chars().any(|c| c != ' ') {
        &raw[1..raw.len() - 1]
    } else {
        raw
    };
    let consumed = match found {
        Some(end) => end + ticks_len - start,
        None => text.len() - start,
    };
    Some((InlineRead::Node(MdInline::Code(trimmed.replace('\n', " "))), consumed))
}

fn read_link<'a>(text: &'a str, start: usize) -> Option<(InlineRead<'a>, usize)> {
    let rest = &text[start + 1..];
    let mut i = 0;
    let bytes = rest.as_bytes();
    let mut close_bracket = None;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if bytes[i] == b']' {
            close_bracket = Some(i);
            break;
        }
        i += 1;
    }
    let close_bracket = close_bracket?;
    let label = &rest[..close_bracket];
    let after_bracket = &rest[close_bracket + 1..];
    let after_open = after_bracket.strip_prefix('(')?;
    let trimmed_open = after_open.trim_start();
    let lead_ws = after_open.len() - trimmed_open.len();

    let (raw_href, after_href) = if let Some(angled) = trimmed_open.strip_prefix('<') {
        let end_angle = angled.find('>')?;
        (&angled[..end_angle], &angled[end_angle + 1..])
    } else {
        let end_href = trimmed_open
            .find(|c: char| c.is_whitespace() || c == ')')
            .unwrap_or(trimmed_open.len());
        if end_href == 0 {
            return None;
        }
        (&trimmed_open[..end_href], &trimmed_open[end_href..])
    };

    let after_href_trimmed = after_href.trim_start();
    let tail = if let Some(quoted) = after_href_trimmed.strip_prefix('"') {
        let end_q = quoted.find('"')?;
        quoted[end_q + 1..].trim_start()
    } else {
        after_href_trimmed
    };
    let after_close = tail.strip_prefix(')')?;
    let consumed = text.len() - start - after_close.len();
    let _ = lead_ws;

    let children = parse_inline(label, false);
    if let Some(href) = safe_href(raw_href) {
        Some((InlineRead::Node(MdInline::Link { href, children }), consumed))
    } else {
        // Unsafe scheme (e.g. javascript:) collapses to plain label text.
        Some((InlineRead::Literal(label), consumed))
    }
}

fn read_autolink<'a>(text: &'a str, start: usize) -> Option<(InlineRead<'a>, usize)> {
    if start > 0 {
        let prev = text[..start].chars().next_back()?;
        if !(prev.is_whitespace() || matches!(prev, '(' | '*' | '_' | '~')) {
            return None;
        }
    }
    let rest = &text[start..];
    if !rest.starts_with("http://") && !rest.starts_with("https://") {
        return None;
    }
    let mut end = 0;
    for (idx, ch) in rest.char_indices() {
        if ch.is_whitespace() || matches!(ch, '<' | '>') {
            break;
        }
        end = idx + ch.len_utf8();
    }
    let mut url = &rest[..end];
    while let Some(last) = url.chars().next_back() {
        if matches!(last, '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' | ')' | ']') {
            url = &url[..url.len() - last.len_utf8()];
        } else {
            break;
        }
    }
    if url.len() <= "https://".len() {
        return None;
    }
    Some((
        InlineRead::Node(MdInline::Link {
            href: url.to_string(),
            children: vec![MdInline::Text(url.to_string())],
        }),
        url.len(),
    ))
}

fn find_emphasis_closer(text: &str, from: usize, mark: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mark_len = mark.len();
    let mark_byte = mark.as_bytes()[0];
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if bytes[i] == b'`' {
            let rel = text[i + 1..].find('`')?;
            i = i + 1 + rel + 1;
            continue;
        }
        if text[i..].starts_with(mark) && i > 0 {
            let prev = text[..i].chars().next_back().unwrap();
            if !prev.is_whitespace() {
                let single = mark_len == 1;
                let doubled = bytes.get(i + 1) == Some(&mark_byte) || (i > 0 && bytes.get(i - 1) == Some(&mark_byte));
                let next_is_word = text[i + mark_len..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_');
                let intraword = mark == "_" && next_is_word;
                if (!single || !doubled) && !intraword {
                    return Some(i);
                }
                if single && bytes.get(i + 1) == Some(&mark_byte) {
                    i += 2;
                    continue;
                }
            }
        }
        let ch = text[i..].chars().next().unwrap();
        i += ch.len_utf8();
    }
    None
}

fn read_emphasis<'a>(text: &'a str, start: usize, partial: bool) -> Option<(InlineRead<'a>, usize)> {
    let bytes = text.as_bytes();
    let ch = bytes[start];
    let mark = if ch == b'~' {
        if bytes.get(start + 1) != Some(&b'~') {
            return None;
        }
        "~~"
    } else if bytes.get(start + 1) == Some(&ch) && bytes.get(start + 2) != Some(&ch) {
        &text[start..start + 2]
    } else {
        &text[start..start + 1]
    };

    let after_mark = start + mark.len();
    let next_ch = text[after_mark..].chars().next();
    let prev_is_word = start > 0
        && text[..start]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
    let can_open = next_ch.is_some_and(|c| !c.is_whitespace()) && !(ch == b'_' && prev_is_word);

    let closer = if can_open {
        find_emphasis_closer(text, after_mark, mark)
    } else {
        None
    };

    if !can_open || (closer.is_none() && !partial) {
        return Some((InlineRead::Literal(mark), mark.len()));
    }

    let inner = match closer {
        Some(end) => &text[after_mark..end],
        None => &text[after_mark..],
    };
    let children = parse_inline(inner, closer.is_none());
    let node = match mark {
        "**" | "__" => MdInline::Strong(children),
        "*" | "_" => MdInline::Em(children),
        "~~" => MdInline::Del(children),
        _ => return None,
    };
    let consumed = match closer {
        Some(end) => end + mark.len() - start,
        None => text.len() - start,
    };
    Some((InlineRead::Node(node), consumed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headings_bullets_code_and_tables() {
        let md = "\
# Architecture Summary

## Key Principles
- **Command registry**: every action flows through `invoke(name, args)`
- **Process isolation**: untrusted code lives in sandboxed services

```rust
let res = core.invoke(Command::TabClose { tab_id: None, force: false });
```

| Crate | Responsibility | Depends on Freya |
| --- | --- | :---: |
| `BrowserCore` | Model, commands | No |
| `UI/Freya` | Native window | Yes |
";

        let blocks = parse_markdown(md, false);
        assert_eq!(blocks.len(), 5);
        assert!(matches!(&blocks[0], MdBlock::Heading { level: 1, .. }));
        assert!(matches!(&blocks[1], MdBlock::Heading { level: 2, .. }));
        match &blocks[2] {
            MdBlock::List { ordered, items, .. } => {
                assert!(!ordered);
                assert_eq!(items.len(), 2);
            }
            other => panic!("expected list, got {other:?}"),
        }
        match &blocks[3] {
            MdBlock::Code { lang, text, open } => {
                assert_eq!(lang.as_deref(), Some("rust"));
                assert!(!open);
                assert!(text.contains("Command::TabClose"));
            }
            other => panic!("expected code block, got {other:?}"),
        }
        match &blocks[4] {
            MdBlock::Table { header, align, rows } => {
                assert_eq!(header.len(), 3);
                assert_eq!(align, &[ColumnAlign::None, ColumnAlign::None, ColumnAlign::Center]);
                assert_eq!(rows.len(), 2);
            }
            other => panic!("expected table, got {other:?}"),
        }
    }

    #[test]
    fn streaming_partial_closes_unterminated_code_and_emphasis() {
        let blocks = parse_markdown("Reading **important `foo", true);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            MdBlock::Paragraph { inlines } => {
                assert_eq!(
                    inlines,
                    &[
                        MdInline::Text("Reading ".to_string()),
                        MdInline::Strong(vec![
                            MdInline::Text("important ".to_string()),
                            MdInline::Code("foo".to_string()),
                        ]),
                    ]
                );
            }
            other => panic!("expected paragraph, got {other:?}"),
        }

        let open_fence = parse_markdown("```rust\nfn main() {", true);
        assert_eq!(
            open_fence,
            vec![MdBlock::Code {
                lang: Some("rust".to_string()),
                text: "fn main() {".to_string(),
                open: true,
            }]
        );
    }

    #[test]
    fn links_allow_only_safe_schemes_and_parse_autolinks() {
        let inlines = parse_inline(
            "See [Ladybird](https://ladybird.org) and [bad](javascript:alert(1)) or https://example.org.",
            false,
        );
        assert!(inlines.iter().any(|n| matches!(
            n,
            MdInline::Link { href, .. } if href == "https://ladybird.org"
        )));
        assert!(inlines.iter().any(|n| matches!(
            n,
            MdInline::Link { href, .. } if href == "https://example.org"
        )));
        assert!(!inlines.iter().any(|n| matches!(
            n,
            MdInline::Link { href, .. } if href.contains("javascript")
        )));
    }
}
