//! Rendering for conflicts required by strict rules, independent of Git's
//! line merger. Only byte-identical three-way boundary lines leave the box;
//! rendering never decides whether a conflict exists.

use super::{safe_label, Labels};
use crate::reason::{MoveEvidence, Side};
use std::collections::BTreeSet;

/// Append file-level move links to the first marker for each affected side.
/// Source content, hunk boundaries and existing reasons remain byte-identical.
pub(crate) fn annotate_moves(content: &str, candidates: &[MoveEvidence], width: usize) -> String {
    let mut notes = [BTreeSet::new(), BTreeSet::new()];
    let index = |side| match side {
        Side::Ours => 0,
        Side::Theirs => 1,
    };
    for candidate in candidates {
        notes[index(candidate.side)].insert(candidate.marker_note());
        notes[index(candidate.opposite_side())].extend(candidate.opposite_marker_notes());
    }
    let suffixes = notes.map(|notes| {
        if notes.is_empty() {
            String::new()
        } else {
            format!(
                " | 文件关联 [analyze]：{}",
                notes.into_iter().collect::<Vec<_>>().join("；")
            )
        }
    });
    let prefixes = [
        format!("{} ⎇ ", "<".repeat(width)),
        format!("{} ⎇ ", ">".repeat(width)),
    ];
    let mut seen = [false; 2];
    let mut result = String::with_capacity(content.len());
    for line in content.split_inclusive('\n') {
        let mut body = line.strip_suffix('\n').unwrap_or(line);
        let ending = if body.ends_with('\r') && line.ends_with('\n') {
            body = &body[..body.len() - 1];
            "\r\n"
        } else if line.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        result.push_str(body);
        for side in 0..2 {
            if !seen[side] && body.starts_with(&prefixes[side]) {
                seen[side] = true;
                if !body.ends_with(&suffixes[side]) {
                    result.push_str(&suffixes[side]);
                }
            }
        }
        result.push_str(ending);
    }
    result
}

/// Wrap the differing middle of three texts in labelled diff3 markers.
///
/// Common prefix/suffix lines appear once outside the box. Line terminators
/// participate in equality, so CRLF and missing final newlines are not erased.
/// Equal changed sides still conflict against base. When all three texts are
/// identical, retain the full box: an external reason may still require review.
/// This is common-context trimming, not Git's zdiff3 merge algorithm.
pub fn conflict_box(
    base: &str,
    ours: &str,
    theirs: &str,
    labels: &Labels,
    width: usize,
    reason: &str,
) -> String {
    let (prefix, [base, ours, theirs], suffix) = trim_context([base, ours, theirs]);
    let section = |text: &str| {
        if text.is_empty() || text.ends_with('\n') {
            text.to_owned()
        } else {
            format!("{text}\n")
        }
    };
    format!(
        "{prefix}{} {} | {}\n{}{} {}\n{}{}\n{}{} {}\n{suffix}",
        "<".repeat(width),
        labels.ours_marker(),
        safe_label(reason),
        section(ours),
        "|".repeat(width),
        labels.base_marker(),
        section(base),
        "=".repeat(width),
        section(theirs),
        ">".repeat(width),
        labels.theirs_marker(),
    )
}

fn trim_context(texts: [&str; 3]) -> (&str, [&str; 3], &str) {
    let [base, ours, theirs] = texts;
    if base == ours && base == theirs {
        return ("", texts, "");
    }
    let mut prefix = 0;
    for ((b, o), t) in base
        .split_inclusive('\n')
        .zip(ours.split_inclusive('\n'))
        .zip(theirs.split_inclusive('\n'))
    {
        if b != o || b != t || !b.ends_with('\n') {
            break;
        }
        prefix += b.len();
    }
    let remaining = texts.map(|text| &text[prefix..]);
    let mut suffix = 0;
    for ((b, o), t) in remaining[0]
        .split_inclusive('\n')
        .rev()
        .zip(remaining[1].split_inclusive('\n').rev())
        .zip(remaining[2].split_inclusive('\n').rev())
    {
        if b != o || b != t {
            break;
        }
        suffix += b.len();
    }
    (
        &base[..prefix],
        texts.map(|text| &text[prefix..text.len() - suffix]),
        &base[base.len() - suffix..],
    )
}
