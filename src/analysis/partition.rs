//! 原文分区与可靠身份：只接受可逐 byte 还原输入的顶层 entity 和间隙。
//! 此模块提供分析前提，不自行产生冲突原因。
use crate::reason::Subject;
use sem_core::parser::plugins::create_default_registry;
use std::collections::BTreeSet;
use weave_core::region::{extract_regions, FileRegion};

#[derive(Debug)]
pub(crate) struct Part {
    pub(crate) key: String,
    pub(super) name: String,
    pub(super) entity_type: String,
    pub(crate) label: String,
    pub(crate) text: String,
    pub(crate) entity: bool,
}

/// Only use a partition when it reproduces every input byte. Nested entities
/// are covered by their outer container in this first implementation.
pub(crate) fn partition(path: &str, text: &str) -> Option<Vec<Part>> {
    // Use the same grammar registry as weave; do not maintain a second
    // extension allowlist. Reliability checks below apply to every language.
    let registry = create_default_registry();
    let plugin = registry.get_explicit_plugin(path)?;
    let (mut entities, tree) = plugin.extract_entities_with_tree(text, path);
    if !text.trim().is_empty() && (tree.as_ref()?.root_node().has_error() || entities.is_empty()) {
        return None;
    }
    entities.sort_by_key(|e| (e.start_line, std::cmp::Reverse(e.end_line)));
    let mut top = Vec::new();
    let mut end = 0;
    for entity in entities {
        if entity.start_line > end {
            end = entity.end_line;
            top.push(entity);
        } else if entity.end_line > end
            || top
                .last()
                .is_some_and(|last: &sem_core::model::entity::SemanticEntity| {
                    last.start_line == entity.start_line && last.end_line == entity.end_line
                })
        {
            return None;
        }
    }
    let regions = extract_regions(text, &top);
    let mut parts = Vec::new();
    let mut seen = BTreeSet::new();
    for region in regions {
        match region {
            FileRegion::Entity(e) => {
                let key = format!("{}:{}", e.entity_type, e.entity_name);
                if !seen.insert(key.clone()) {
                    return None;
                }
                parts.push(Part {
                    key,
                    label: format!("{} {}", e.entity_type, e.entity_name),
                    name: e.entity_name,
                    entity_type: e.entity_type,
                    text: e.content,
                    entity: true,
                });
            }
            FileRegion::Interstitial(gap) => {
                parts.push(Part {
                    key: format!("gap:{}", parts.len()),
                    name: String::new(),
                    entity_type: String::new(),
                    label: "非实体文本".into(),
                    text: gap.content,
                    entity: false,
                });
            }
        }
    }
    if parts.iter().map(|p| p.text.as_str()).collect::<String>() != text {
        return None;
    }
    // Interstitial regions alternate with entities. Describe their position
    // using reliable neighbours, without interpreting the region's contents.
    for index in 0..parts.len() {
        if parts[index].entity {
            continue;
        }
        let previous = index
            .checked_sub(1)
            .and_then(|i| parts.get(i))
            .filter(|p| p.entity);
        let next = parts.get(index + 1).filter(|p| p.entity);
        parts[index].label = match (previous, next) {
            (None, Some(next)) => format!("文件开头、{} 之前的非实体区域", next.label),
            (Some(previous), None) => format!("文件末尾、{} 之后的非实体区域", previous.label),
            (Some(previous), Some(next)) => {
                format!("{} 与 {} 之间的非实体区域", previous.label, next.label)
            }
            (None, None) => "文件中的非实体区域".into(),
        };
    }
    Some(parts)
}

impl Part {
    pub(super) fn subject(&self) -> Subject {
        if self.entity {
            Subject::Entity {
                entity_type: self.entity_type.clone(),
                name: self.name.clone(),
            }
        } else {
            Subject::Gap {
                key: self.key.clone(),
                label: self.label.clone(),
            }
        }
    }
}

pub(super) fn same_keys(x: &[Part], y: &[Part]) -> bool {
    x.iter().map(|p| &p.key).eq(y.iter().map(|p| &p.key))
}

// The public weave classification exposes a name, not a full identity. Only
// bind it when all three partitions are reliable and agree on a unique target.
pub(super) type ThreePartitions = (Option<Vec<Part>>, Option<Vec<Part>>, Option<Vec<Part>>);

pub(super) fn local_subject(
    parts: &ThreePartitions,
    name: &str,
    entity_type: Option<&str>,
) -> Option<Subject> {
    let (Some(b), Some(o), Some(t)) = parts else {
        return None;
    };
    let mut candidates = BTreeSet::new();
    for version in [b, o, t] {
        let found: Vec<_> = version
            .iter()
            .filter(|p| {
                p.entity && p.name == name && entity_type.is_none_or(|kind| p.entity_type == kind)
            })
            .collect();
        if found.len() > 1 {
            return None;
        }
        candidates.extend(found.into_iter().map(Part::subject));
    }
    if candidates.len() == 1 {
        candidates.pop_first()
    } else {
        None
    }
}

// Only compact the layout fallback when every original byte is unchanged and
// both appended tails contain distinct new entities and whitespace. Other
// layout changes and strict entity conflicts keep their full three-way scope.
pub(super) fn distinct_entity_appends(base: &str, ours: &str, theirs: &str, path: &str) -> bool {
    let Some(base_parts) = partition(path, base) else {
        return false;
    };
    let base_keys: BTreeSet<_> = base_parts
        .iter()
        .filter(|p| p.entity)
        .map(|p| p.key.as_str())
        .collect();
    let added = |text: &str| -> Option<BTreeSet<String>> {
        let tail = text.strip_prefix(base)?;
        let parts = partition(path, tail)?;
        if parts.iter().any(|p| {
            (!p.entity && !p.text.trim().is_empty())
                || (p.entity && base_keys.contains(p.key.as_str()))
        }) {
            return None;
        }
        let keys: BTreeSet<_> = parts
            .into_iter()
            .filter(|p| p.entity)
            .map(|p| p.key)
            .collect();
        (!keys.is_empty()).then_some(keys)
    };
    matches!((added(ours), added(theirs)), (Some(o), Some(t)) if o.is_disjoint(&t))
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    fn part(kind: &str, name: &str) -> Part {
        Part {
            key: format!("{kind}:{name}"),
            name: name.into(),
            entity_type: kind.into(),
            label: format!("{kind} {name}"),
            text: String::new(),
            entity: true,
        }
    }
    #[test]
    fn public_name_cannot_bind_ambiguous_types_or_unreliable_versions() {
        let parts = (
            Some(vec![part("function", "f"), part("interface", "f")]),
            Some(vec![part("function", "f")]),
            Some(vec![part("interface", "f")]),
        );
        assert!(local_subject(&parts, "f", None).is_none());
        assert_eq!(
            local_subject(&parts, "f", Some("function")),
            Some(part("function", "f").subject())
        );
        let parts = (
            Some(vec![part("function", "f")]),
            None,
            Some(vec![part("function", "f")]),
        );
        assert!(local_subject(&parts, "f", Some("function")).is_none());
    }
    #[test]
    fn type_changes_are_not_merged_but_confirmed_modify_delete_can_share_parent() {
        let parts = (
            Some(vec![part("function", "f")]),
            Some(vec![part("class", "f")]),
            Some(vec![part("function", "f")]),
        );
        assert!(local_subject(&parts, "f", None).is_none());
        let parts = (
            Some(vec![part("function", "f")]),
            Some(vec![]),
            Some(vec![part("function", "f")]),
        );
        assert_eq!(
            local_subject(&parts, "f", None),
            Some(part("function", "f").subject())
        );
    }
}
