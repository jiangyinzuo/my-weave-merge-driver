//! Rendering for conflicts required by strict rules, independent of Git's
//! line merger. Only byte-identical three-way boundary lines leave the box;
//! rendering never decides whether a conflict exists.

use super::{safe_label, Labels};

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
        "{prefix}{} ours: {} | {}\n{}{} base: {}\n{}{}\n{}{} theirs: {}\n{suffix}",
        "<".repeat(width),
        safe_label(&labels.ours),
        safe_label(reason),
        section(ours),
        "|".repeat(width),
        safe_label(&labels.base),
        section(base),
        "=".repeat(width),
        section(theirs),
        ">".repeat(width),
        safe_label(&labels.theirs),
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
