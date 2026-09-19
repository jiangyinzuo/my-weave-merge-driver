//! 跨文件移动候选：逐侧比较 deleted/added 的类型、名称和原文，保留全部歧义。
//! 匹配只读取三份快照，不写报告或运行 Git。
use super::partition::partition;
use crate::reason::{Evidence, Kind, Location, MoveEvidence, Opposite, Reason, Side, Subject};
use anyhow::{bail, Result};
use std::collections::BTreeMap;

pub(super) struct Entity {
    key: String,
    location: Location,
    text: String,
}
pub(super) type Entities = BTreeMap<String, Entity>;

pub(super) fn entities(path: &str, text: &str) -> Option<Entities> {
    let mut result = BTreeMap::new();
    let mut line = 1;
    for part in partition(path, text)? {
        let next = line + part.text.bytes().filter(|b| *b == b'\n').count();
        if part.entity {
            result.insert(
                part.key.clone(),
                Entity {
                    key: part.key,
                    location: Location {
                        path: path.into(),
                        entity: part.label,
                        line,
                    },
                    text: part.text,
                },
            );
        }
        line = next;
    }
    Some(result)
}

/// None 表示解析失败，空 map 表示已确认不存在 entity。
pub(super) type ParsedSnapshots = [BTreeMap<String, Option<Entities>>; 3];

pub(super) fn find_candidates(parsed: &ParsedSnapshots) -> Result<Vec<MoveEvidence>> {
    let mut candidates = Vec::new();
    for (side, direction) in [(1, Side::Ours), (2, Side::Theirs)] {
        let other = 3 - side;
        let mut deleted: BTreeMap<(&str, &str), Vec<&Entity>> = BTreeMap::new();
        let mut added: BTreeMap<(&str, &str), Vec<&Entity>> = BTreeMap::new();
        for (path, base) in &parsed[0] {
            let (Some(base), Some(current)) = (base.as_ref(), parsed[side][path].as_ref()) else {
                continue;
            };
            for (key, entity) in base {
                if !current.contains_key(key) {
                    deleted
                        .entry((&entity.key, &entity.text))
                        .or_default()
                        .push(entity);
                }
            }
            for (key, entity) in current {
                if !base.contains_key(key) {
                    added
                        .entry((&entity.key, &entity.text))
                        .or_default()
                        .push(entity);
                }
            }
        }
        for (key, sources) in deleted {
            let Some(targets) = added.get(&key) else {
                continue;
            };
            if sources.len().saturating_mul(targets.len()) > 10_000 - candidates.len() {
                bail!("移动候选超过 10000，无法完整生成报告；未写出分析文件");
            }
            for source in &sources {
                let opposite = match parsed[other][&source.location.path].as_ref() {
                    Some(entities) => match entities.get(&source.key) {
                        Some(entity) if entity.text == source.text => Opposite::Unchanged,
                        Some(_) => Opposite::Modified,
                        None => Opposite::Deleted,
                    },
                    None => Opposite::Unknown,
                };
                for target in targets {
                    if source.location.path == target.location.path {
                        continue;
                    }
                    let candidate = MoveEvidence {
                        side: direction,
                        base: source.location.clone(),
                        target: target.location.clone(),
                        source_count: sources.len(),
                        destination_count: targets.len(),
                        opposite,
                    };
                    candidates.push(candidate);
                }
            }
        }
    }
    Ok(candidates)
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
                candidate: candidate.clone(),
            },
        )
    })
}
