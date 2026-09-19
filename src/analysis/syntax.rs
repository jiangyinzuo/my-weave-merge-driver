//! Formatting-insensitive candidate keys, never a merge or semantic-equivalence proof.
//! Reuse sem-core's public parser. Preserve structure, field names, token order,
//! comments and literal bytes; ignore only whitespace gaps outside syntax nodes.
use sem_core::parser::plugins::code::{language_config_for_content, parse_tree};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Syntax(Vec<Atom>);

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Atom {
    Open(String, Option<String>, bool),
    Text(String),
    Close,
}

impl Syntax {
    pub(super) fn token_count(&self) -> usize {
        self.0.iter().filter(|a| matches!(a, Atom::Text(_))).count()
    }
}

/// Compare the full ordered representation directly, without hash equality.
/// A standalone entity that cannot be parsed does not get this fallback.
pub(super) fn normalized_syntax(path: &str, text: &str) -> Option<Syntax> {
    let tree = parse_tree(language_config_for_content(text, path)?, text)?;
    if tree.root_node().has_error() {
        return None;
    }
    let mut cursor = tree.walk();
    let mut atoms = Vec::new();
    let mut covered = 0;
    loop {
        let node = cursor.node();
        if node.is_missing() || node.is_error() {
            return None;
        }
        let kind = node.kind();
        // Keep literal/comment subtrees intact, including whitespace that a
        // grammar may leave between their children (e.g. interpolated strings).
        let opaque = ["string", "char", "comment", "regex", "template", "text"]
            .iter()
            .any(|part| kind.contains(part));
        atoms.push(Atom::Open(
            kind.into(),
            cursor.field_name().map(str::to_owned),
            node.is_named(),
        ));
        if !opaque && cursor.goto_first_child() {
            continue;
        }
        let start = node.start_byte();
        let end = node.end_byte();
        // Reject overlaps or uncovered non-whitespace rather than silently
        // discarding bytes the grammar did not account for.
        if start < covered
            || !text[covered..start]
                .bytes()
                .all(|b| b.is_ascii_whitespace())
        {
            return None;
        }
        atoms.push(Atom::Text(text[start..end].into()));
        covered = end;
        loop {
            atoms.push(Atom::Close);
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return text[covered..]
                    .bytes()
                    .all(|b| b.is_ascii_whitespace())
                    .then_some(Syntax(atoms));
            }
        }
    }
}
