//! 跨文件移动及重命名候选：先匹配原文，再匹配同 grammar/type 的名称归一化原文。
//! 匹配只读取三份快照，不写报告或运行 Git。
use super::partition::Part;
use super::syntax::{normalized_syntax, Syntax};
use crate::reason::{
    Evidence, Kind, Location, MoveEvidence, MoveMatch, Opposite, OppositeMove, Reason,
    RenameComparison, Side, Subject,
};
use anyhow::{bail, Result};
use sem_core::parser::plugins::code::language_config_for_content;
use std::collections::{BTreeMap, BTreeSet};
use weave_core::binding::replace_at_word_boundaries;

const NAME_SENTINEL: &str = "__ENTITY__";
const MAX_CANDIDATES: usize = 10_000;

pub(super) struct Entity {
    key: String,
    name: String,
    entity_type: String,
    grammar: Option<&'static str>,
    location: Location,
    text: String,
}
pub(super) type Entities = BTreeMap<String, Entity>;
type ExactBucket<'a> = (Vec<&'a Entity>, Vec<&'a Entity>);

pub(super) fn entities(path: &str, text: &str, parts: Vec<Part>) -> Entities {
    let mut result = BTreeMap::new();
    let mut line = 1;
    let grammar = language_config_for_content(text, path).map(|config| config.id);
    for part in parts {
        let next = line + part.text.bytes().filter(|b| *b == b'\n').count();
        if part.entity {
            result.insert(
                part.key.clone(),
                Entity {
                    key: part.key,
                    name: part.name.clone(),
                    entity_type: part.entity_type.clone(),
                    grammar,
                    location: Location {
                        path: path.into(),
                        entity_type: part.entity_type,
                        name: part.name,
                        line,
                    },
                    text: part.text,
                },
            );
        }
        line = next;
    }
    result
}

/// None 表示解析失败，空 map 表示已确认不存在 entity。
pub(super) type ParsedSnapshots = [BTreeMap<String, Option<Entities>>; 3];

/// 不复用上游的一对一贪心选择：精确和名称归一化候选全部保留。
pub(super) fn find_candidates(parsed: &ParsedSnapshots) -> Result<Vec<MoveEvidence>> {
    let mut candidates = Vec::new();
    for (side, direction) in [(1, Side::Ours), (2, Side::Theirs)] {
        let mut deleted = Vec::new();
        let mut added = Vec::new();
        for (path, base) in &parsed[0] {
            let (Some(base), Some(current)) = (base.as_ref(), parsed[side][path].as_ref()) else {
                continue;
            };
            deleted.extend(
                base.iter()
                    .filter(|(key, _)| !current.contains_key(*key))
                    .map(|(_, e)| e),
            );
            added.extend(
                current
                    .iter()
                    .filter(|(key, _)| !base.contains_key(*key))
                    .map(|(_, e)| e),
            );
        }
        // 保留已有同名原文规则，包括其语言支持范围。
        let mut exact: BTreeMap<(&str, &str), ExactBucket<'_>> = BTreeMap::new();
        for entity in &deleted {
            exact
                .entry((&entity.key, &entity.text))
                .or_default()
                .0
                .push(entity);
        }
        for entity in &added {
            exact
                .entry((&entity.key, &entity.text))
                .or_default()
                .1
                .push(entity);
        }
        for (sources, targets) in exact.values() {
            check_bucket_limit(sources.len(), targets.len(), candidates.len())?;
            for source in sources {
                for target in targets {
                    if source.location.path != target.location.path {
                        candidates.push(candidate(
                            parsed,
                            direction,
                            source,
                            target,
                            MoveMatch::Exact,
                        ));
                    }
                }
            }
        }
        add_rename_candidates(parsed, direction, &deleted, &added, &mut candidates)?;
    }
    // 数量表达实际关联的不同候选，不含同路径或同名过滤掉的组合。
    // 同名移动与 rename 同时成立时，不隐藏任何一方的歧义。
    candidates.sort();
    candidates.dedup();
    let mut sources = BTreeMap::new();
    let mut targets = BTreeMap::new();
    for c in &candidates {
        *sources.entry((c.side, c.target.clone())).or_insert(0) += 1;
        *targets.entry((c.side, c.base.clone())).or_insert(0) += 1;
    }
    for c in &mut candidates {
        c.source_count = sources[&(c.side, c.target.clone())];
        c.destination_count = targets[&(c.side, c.base.clone())];
    }
    let mut by_source: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for c in &candidates {
        by_source
            .entry((c.side, c.base.clone()))
            .or_default()
            .push(OppositeMove {
                target: c.target.clone(),
                matched_by: c.matched_by.clone(),
                source_count: c.source_count,
                destination_count: c.destination_count,
            });
    }
    for others in by_source.values_mut() {
        others.sort();
        others.dedup();
    }
    let mut linked_count = 0;
    for c in &mut candidates {
        let count = by_source
            .get(&(c.opposite_side(), c.base.clone()))
            .map_or(0, Vec::len);
        check_bucket_limit(1, count, linked_count)?;
        linked_count += count;
        c.opposite_moves = by_source
            .get(&(c.opposite_side(), c.base.clone()))
            .cloned()
            .unwrap_or_default();
    }
    Ok(candidates)
}

/// 拒绝截断报告。上限按候选 bucket 的潜在组合检查，可能比最终候选数更保守。
fn check_bucket_limit(sources: usize, targets: usize, existing: usize) -> Result<()> {
    if sources.saturating_mul(targets) > MAX_CANDIDATES - existing {
        bail!("移动候选超过 10000，无法完整生成报告；未写出分析文件");
    }
    Ok(())
}

/// 仅复用公开的词边界替换，不依赖私有 hash/相似度或 Debug 输出。
fn normalized(entity: &Entity) -> Option<(String, usize)> {
    if entity.name.is_empty() || entity.text.contains(NAME_SENTINEL) {
        return None; // 避免真实文本中的 sentinel 伪造名称归一化匹配。
    }
    let text = replace_at_word_boundaries(&entity.text, &entity.name, NAME_SENTINEL);
    let occurrences = text.matches(NAME_SENTINEL).count();
    (occurrences > 0).then_some((text, occurrences))
}

type RenameBucket<'a> = (Vec<(&'a Entity, usize)>, Vec<(&'a Entity, usize)>);

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum RenameKey {
    Text(String),
    Syntax(Syntax),
}

fn add_rename_candidates(
    parsed: &ParsedSnapshots,
    side: Side,
    deleted: &[&Entity],
    added: &[&Entity],
    candidates: &mut Vec<MoveEvidence>,
) -> Result<()> {
    let mut buckets: BTreeMap<(&str, &str, RenameKey), RenameBucket<'_>> = BTreeMap::new();
    for (is_added, entities) in [(false, deleted), (true, added)] {
        for entity in entities {
            let (Some(grammar), Some((text, count))) = (entity.grammar, normalized(entity)) else {
                continue;
            };
            let syntax = normalized_syntax(&entity.location.path, &text).map(RenameKey::Syntax);
            for key in std::iter::once(RenameKey::Text(text)).chain(syntax) {
                let bucket = buckets
                    .entry((grammar, &entity.entity_type, key))
                    .or_default();
                if is_added {
                    bucket.1.push((entity, count));
                } else {
                    bucket.0.push((entity, count));
                }
            }
        }
    }
    let mut text_pairs = BTreeSet::new();
    for ((grammar, entity_type, key), (sources, targets)) in buckets {
        check_bucket_limit(sources.len(), targets.len(), candidates.len())?;
        for (source, old_occurrences) in &sources {
            for (target, new_occurrences) in &targets {
                if source.location.path == target.location.path || source.name == target.name {
                    continue;
                }
                // A text match already provides stronger evidence for this
                // pair. Do not count a syntax fallback as a second candidate.
                if matches!(key, RenameKey::Syntax(_))
                    && text_pairs.contains(&(&source.location, &target.location))
                {
                    continue;
                }
                let comparison = match &key {
                    RenameKey::Text(_) => {
                        text_pairs.insert((&source.location, &target.location));
                        RenameComparison::Text
                    }
                    RenameKey::Syntax(syntax) => RenameComparison::Syntax {
                        tokens: syntax.token_count(),
                    },
                };
                candidates.push(candidate(
                    parsed,
                    side,
                    source,
                    target,
                    MoveMatch::NameNormalized {
                        grammar: grammar.into(),
                        entity_type: entity_type.into(),
                        old_name: source.name.clone(),
                        new_name: target.name.clone(),
                        old_occurrences: *old_occurrences,
                        new_occurrences: *new_occurrences,
                        comparison,
                    },
                ));
            }
        }
    }
    Ok(())
}

fn candidate(
    parsed: &ParsedSnapshots,
    side: Side,
    source: &Entity,
    target: &Entity,
    matched_by: MoveMatch,
) -> MoveEvidence {
    let other = match side {
        Side::Ours => 2,
        Side::Theirs => 1,
    };
    let opposite = match parsed[other][&source.location.path].as_ref() {
        Some(entities) => match entities.get(&source.key) {
            Some(entity) if entity.text == source.text => Opposite::Unchanged,
            Some(_) => Opposite::Modified,
            None => Opposite::Deleted,
        },
        None => Opposite::Unknown,
    };
    MoveEvidence {
        side,
        base: source.location.clone(),
        target: target.location.clone(),
        source_count: 1,
        destination_count: 1,
        opposite,
        matched_by,
        opposite_moves: Vec::new(),
    }
}

/// 纯移动只关联；另一侧修改、删除或无法确认未变时，要求源与目标共同审核。
pub(super) fn conflict_reason(candidate: &MoveEvidence) -> Option<Reason> {
    (candidate.opposite != Opposite::Unchanged).then(|| {
        Reason::new(
            Kind::ModifyVsMove,
            Subject::Move {
                side: candidate.side,
                base: candidate.base.clone(),
                target: candidate.target.clone(),
            },
            Evidence::MoveCandidate {
                candidate: Box::new(candidate.clone()),
            },
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_limit_is_shared_and_never_silently_truncates() {
        assert!(check_bucket_limit(100, 100, 0).is_ok());
        assert!(check_bucket_limit(100, 100, 1).is_err());
        assert!(check_bucket_limit(101, 100, 0).is_err());
        assert!(check_bucket_limit(usize::MAX, 2, 0).is_err());
    }
}
