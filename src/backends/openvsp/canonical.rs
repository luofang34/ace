//! Deterministic OpenVSP artifact text. The kernel assigns random object identifiers, so
//! identical canonical inputs would otherwise produce different content addresses and break
//! evidence replay.

use std::collections::HashMap;

const ID_LENGTH: std::ops::RangeInclusive<usize> = 8..=12;

/// Rename every object identifier in order of first occurrence and order identifier-keyed lists.
pub(crate) fn vsp3(text: &str) -> String {
    sort_unsteady_components(&rename_ids(text))
}

/// Identifiers appear as `ID="…"` attributes (including `AttachID`) and `<…ID>…</…ID>` elements.
/// Every match is adjacent to ASCII delimiters, so the byte offsets are char boundaries.
fn rename_ids(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut mapping: HashMap<&str, String> = HashMap::new();
    let mut result = String::with_capacity(text.len());
    let mut copied = 0;
    let mut index = 0;
    while index < bytes.len() {
        let rest = &bytes[index..];
        let (prefix, terminator) = if rest.starts_with(b"ID=\"") {
            (4, b'"')
        } else if rest.starts_with(b"ID>") {
            (3, b'<')
        } else {
            index += 1;
            continue;
        };
        let start = index + prefix;
        let length = bytes[start..]
            .iter()
            .take_while(|byte| byte.is_ascii_uppercase())
            .count();
        let end = start + length;
        if ID_LENGTH.contains(&length) && bytes.get(end) == Some(&terminator) {
            let token = &text[start..end];
            let next = mapping.len();
            let canonical = mapping.entry(token).or_insert_with(|| encode(next, length));
            result.push_str(&text[copied..start]);
            result.push_str(canonical);
            copied = end;
            index = end;
        } else {
            index = start;
        }
    }
    result.push_str(&text[copied..]);
    result
}

/// Fixed-width base-26 upper-case encoding, so renamed identifiers keep the kernel's format.
fn encode(mut value: usize, length: usize) -> String {
    let mut digits = vec![b'A'; length];
    for digit in digits.iter_mut().rev() {
        *digit = b'A' + (value % 26) as u8;
        value /= 26;
    }
    String::from_utf8(digits).unwrap_or_default()
}

/// VSPAERO unsteady groups list their components in identifier order; the set is unordered.
fn sort_unsteady_components(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<Unsteady_Group>") {
        let Some(length) = rest[start..].find("</Unsteady_Group>") else {
            break;
        };
        let end = start + length;
        result.push_str(&rest[..start]);
        result.push_str(&sort_component_blocks(&rest[start..end]));
        rest = &rest[end..];
    }
    result.push_str(rest);
    result
}

fn sort_component_blocks(group: &str) -> String {
    let mut result = String::with_capacity(group.len());
    let mut blocks: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for line in group.split_inclusive('\n') {
        let content = line.trim();
        match current.as_mut() {
            Some(block) => {
                block.push_str(line);
                if content == "</Component>" {
                    blocks.extend(current.take());
                }
            }
            None if content == "<Component>" => current = Some(line.to_owned()),
            None => {
                blocks.sort();
                result.extend(blocks.drain(..));
                result.push_str(line);
            }
        }
    }
    blocks.sort();
    result.extend(blocks.drain(..));
    result.extend(current);
    result
}

#[cfg(test)]
mod tests;
