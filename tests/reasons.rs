use strict_weave::reason::{
    self, Change, Evidence, Kind, Location, MoveEvidence, MoveMatch, Opposite, Reason, Side,
    Subject, WeaveRefusal, WeaveSource,
};
use weave_core::conflict::ConflictKind;

const REASON_PROVENANCE_TXT: &str = r#"原因 [weave]：ENTITY_CONFLICT：ƒ f 需人工审核
  依据 [weave]：分类：ours=deleted, theirs=modified
  依据 [weave]：拒绝：modify_delete；modified in theirs
"#;
const REASON_UNAVAILABLE_TXT: &str = r#"原因 [analyze]：ENTITY_ANALYSIS_UNAVAILABLE：无法可靠分析，保留整文件冲突
  依据 [analyze]：weave 未返回实体分类
"#;
const REASON_DELETE_RENAME_TXT: &str = r#"原因 [weave]：ENTITY_CONFLICT：ƒ f 需人工审核
  依据 [weave]：分类：ours=deleted, theirs=rename + modified candidate
"#;
const REASON_UNKNOWN_EVIDENCE_JSON: &str = r#"{"kind":"line_conflict","subject":{"type":"file"},"evidence":[{"type":"unknown"}]}
"#;
const REASON_UNKNOWN_KIND_JSON: &str = r#"{"kind":"unknown","subject":{"type":"file"},"evidence":[]}
"#;
const REASON_CONTROLS_JSON: &str = r#"{"kind":"entity_conflict","subject":{"type":"entity","entity_type":"function","name":"f\n\u001b[31m"},"evidence":[{"type":"weave_refusal","refusal":{"kind":"rename_modify","old_name":"a\nb","new_name":"c\rd","renamed_in":"ours"}}]}"#;
const REASON_UNKNOWN_FIELD_JSON: &str = r#"{"kind":"line_conflict","subject":{"type":"file"},"evidence":[],"surprise":1}
"#;
const REASON_ACTIONS_TXT: &str = r#"原因 [weave]：ENTITY_CONFLICT：ƒ f 需人工审核
  依据 [weave]：分类：ours=modified, theirs=modified
"#;
const REASON_ACTIONS_DETAILS_TXT: &str = r#"原因 [weave]：ENTITY_CONFLICT：ƒ f 需人工审核
  依据 [weave]：分类：ours=modified, theirs=modified
"#;

fn entity(name: &str) -> Subject {
    Subject::Entity {
        entity_type: "function".into(),
        name: name.into(),
    }
}

#[test]
fn function_glyph_uses_structured_entity_type_and_preserves_function_words_in_names() {
    let reason = Reason::new(
        Kind::EntityConflict,
        entity("functionality"),
        Evidence::RawBothChanged,
    );
    assert_eq!(
        reason.summary(),
        "ENTITY_CONFLICT：ƒ functionality 需人工审核"
    );
    assert!(!reason.summary().contains("function function"));
}

#[test]
fn entity_type_glyphs_are_stable_and_unknown_types_remain_visible() {
    let examples = [
        ("class", "◇"),
        ("interface", "◇"),
        ("struct", "◇"),
        ("enum", "◇"),
        ("method", "◇"),
        ("module", "◇"),
        ("macro_definition", "◇"),
        ("value_declaration", "◇"),
        ("test", "◇"),
        ("future_entity_type", "◇"),
    ];
    for (entity_type, display) in examples {
        assert_eq!(
            strict_weave::reason::human_entity_type(entity_type),
            display,
            "{entity_type}"
        );
    }
}

fn movement(opposite: Opposite) -> MoveEvidence {
    MoveEvidence {
        side: Side::Theirs,
        base: Location {
            path: "old.go".into(),
            entity_type: "function".into(),
            name: "f".into(),
            line: 1,
        },
        target: Location {
            path: "new.go".into(),
            entity_type: "function".into(),
            name: "f".into(),
            line: 4,
        },
        source_count: 1,
        destination_count: 2,
        opposite,
        matched_by: MoveMatch::Exact,
        opposite_moves: Vec::new(),
    }
}
fn move_reason(c: MoveEvidence) -> Reason {
    Reason::new(
        Kind::ModifyVsMove,
        Subject::Move {
            side: c.side,
            base: c.base.clone(),
            target: c.target.clone(),
        },
        Evidence::MoveCandidate {
            candidate: Box::new(c),
        },
    )
}
fn refusals() -> Vec<ConflictKind> {
    vec![
        ConflictKind::BothModified,
        ConflictKind::BothAdded,
        ConflictKind::ModifyDelete {
            modified_in_ours: true,
        },
        ConflictKind::ModifyDelete {
            modified_in_ours: false,
        },
        ConflictKind::RenameRename {
            base_name: "f".into(),
            ours_name: "g".into(),
            theirs_name: "h".into(),
        },
        ConflictKind::RenameModify {
            old_name: "f".into(),
            new_name: "g".into(),
            renamed_in_ours: true,
        },
        ConflictKind::RenameModify {
            old_name: "f".into(),
            new_name: "g".into(),
            renamed_in_ours: false,
        },
    ]
}

#[test]
fn complete_parent_and_evidence_catalog_roundtrips_without_loss() {
    let mut reasons = vec![
        Reason::new(Kind::LineConflict, Subject::File, Evidence::GitLineConflict),
        Reason::new(Kind::EntityConflict, entity("f"), Evidence::RawBothChanged),
        Reason::new(Kind::EntityConflict, entity("f"), Evidence::IdenticalEdits),
        Reason::new(
            Kind::NonEntityConflict,
            Subject::Gap {
                key: "gap:0".into(),
                label: "文件开头、function f 之前的非实体区域".into(),
            },
            Evidence::RawBothChanged,
        ),
        Reason::new(
            Kind::AnalysisUnavailable,
            Subject::File,
            Evidence::PartitionUnavailable,
        ),
        Reason::new(
            Kind::AnalysisUnavailable,
            Subject::File,
            Evidence::WeaveUnavailable,
        ),
        Reason::new(Kind::LayoutChanged, Subject::File, Evidence::LayoutChanged),
    ];
    for opposite in [Opposite::Modified, Opposite::Deleted, Opposite::Unknown] {
        reasons.push(move_reason(movement(opposite)));
    }
    for refusal in refusals() {
        reasons.push(Reason::new(
            Kind::EntityConflict,
            entity("f"),
            Evidence::WeaveRefusal {
                refusal: (&refusal).into(),
            },
        ));
    }
    for ours in [
        Change::Added,
        Change::Deleted,
        Change::Modified,
        Change::Renamed,
        Change::RenameModified,
    ] {
        for theirs in [
            Change::Added,
            Change::Deleted,
            Change::Modified,
            Change::Renamed,
            Change::RenameModified,
        ] {
            reasons.push(Reason::new(
                Kind::EntityConflict,
                entity("f"),
                Evidence::WeaveActions { ours, theirs },
            ));
        }
    }
    for reason in &reasons {
        assert!(reason.valid(), "{reason:?}");
        let json = serde_json::to_string(reason).unwrap();
        let decoded: Reason = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, *reason);
        assert_eq!(reason.lines(true).len(), reason.evidence.len() + 1);
        assert!(!reason.summary().is_empty());
    }
    // Three states on the same move subject remain separate evidence, not guesses.
    reason::normalize(&mut reasons);
    assert_eq!(reasons.len(), 6);
    let entity = reasons
        .iter()
        .find(|r| r.kind == Kind::EntityConflict)
        .unwrap();
    assert_eq!(entity.evidence.len(), 34); // raw + identical + 25 action pairs + 7 refusal variants/directions
    assert_eq!(
        reasons
            .iter()
            .find(|r| r.kind == Kind::ModifyVsMove)
            .unwrap()
            .evidence
            .len(),
        3
    );
}

#[test]
fn grouping_is_order_independent_idempotent_and_preserves_distinct_targets() {
    let mut reasons = vec![
        Reason::new(Kind::EntityConflict, entity("f"), Evidence::RawBothChanged),
        Reason::new(
            Kind::EntityConflict,
            entity("f"),
            Evidence::WeaveActions {
                ours: Change::Modified,
                theirs: Change::Modified,
            },
        ),
        Reason::new(Kind::EntityConflict, entity("g"), Evidence::RawBothChanged),
        Reason::new(
            Kind::EntityConflict,
            Subject::Entity {
                entity_type: "interface".into(),
                name: "f".into(),
            },
            Evidence::RawBothChanged,
        ),
        Reason::new(Kind::LineConflict, Subject::File, Evidence::GitLineConflict),
    ];
    for ordinal in 0..2 {
        reasons.push(Reason::new(
            Kind::EntityConflict,
            Subject::Weave {
                name: "f".into(),
                source: WeaveSource::Classification,
                ordinal,
            },
            Evidence::WeaveActions {
                ours: Change::Modified,
                theirs: Change::Modified,
            },
        ));
    }
    for key in ["gap:0", "gap:2"] {
        reasons.push(Reason::new(
            Kind::NonEntityConflict,
            Subject::Gap {
                key: key.into(),
                label: "文件中的非实体区域".into(),
            },
            Evidence::RawBothChanged,
        ));
    }
    let mut reversed = reasons.clone();
    reversed.reverse();
    reversed.extend(reasons.clone());
    reason::normalize(&mut reasons);
    reason::normalize(&mut reversed);
    assert_eq!(reasons, reversed);
    assert_eq!(reasons.len(), 8);
    let first = reasons.clone();
    reason::normalize(&mut reasons);
    assert_eq!(reasons, first);
    assert_eq!(
        reasons
            .iter()
            .find(|r| r.subject == entity("f"))
            .unwrap()
            .evidence
            .len(),
        2
    );
}

#[test]
fn summary_hides_repeated_support_but_keeps_distinct_actions_and_refusals() {
    let reason = Reason::new(
        Kind::EntityConflict,
        entity("f"),
        Evidence::WeaveActions {
            ours: Change::Modified,
            theirs: Change::Modified,
        },
    );
    assert_eq!(
        reason.lines(false),
        REASON_ACTIONS_TXT.lines().collect::<Vec<_>>()
    );
    assert_eq!(
        reason.lines(true),
        REASON_ACTIONS_DETAILS_TXT.lines().collect::<Vec<_>>()
    );
    let before = reason.clone();
    reason.lines(false);
    assert_eq!(reason, before);
    for refusal in refusals() {
        let r = Reason::new(
            Kind::EntityConflict,
            entity("f"),
            Evidence::WeaveRefusal {
                refusal: (&refusal).into(),
            },
        );
        assert_eq!(r.lines(false).len(), 2);
    }
    let r = Reason::new(
        Kind::EntityConflict,
        entity("f"),
        Evidence::WeaveActions {
            ours: Change::Deleted,
            theirs: Change::RenameModified,
        },
    );
    assert_eq!(
        r.lines(false),
        REASON_DELETE_RENAME_TXT.lines().collect::<Vec<_>>()
    );
}

#[test]
fn every_upstream_refusal_retains_direction_and_rename_names() {
    assert_eq!(
        WeaveRefusal::from(&ConflictKind::ModifyDelete {
            modified_in_ours: false
        }),
        WeaveRefusal::ModifyDelete {
            modified_in: Side::Theirs
        }
    );
    assert_eq!(
        WeaveRefusal::from(&ConflictKind::RenameModify {
            old_name: "old".into(),
            new_name: "new".into(),
            renamed_in_ours: true
        }),
        WeaveRefusal::RenameModify {
            old_name: "old".into(),
            new_name: "new".into(),
            renamed_in: Side::Ours
        }
    );
    assert_eq!(
        WeaveRefusal::from(&ConflictKind::RenameRename {
            base_name: "base".into(),
            ours_name: "ours".into(),
            theirs_name: "theirs".into()
        }),
        WeaveRefusal::RenameRename {
            base_name: "base".into(),
            ours_name: "ours".into(),
            theirs_name: "theirs".into()
        }
    );
}

#[test]
fn invalid_empty_unknown_and_nonblocking_evidence_is_rejected() {
    let actions = [
        Change::Added,
        Change::Absent,
        Change::Deleted,
        Change::Unchanged,
        Change::Modified,
        Change::Renamed,
        Change::RenameModified,
    ];
    for ours in actions {
        for theirs in actions {
            let expected = !matches!(ours, Change::Absent | Change::Unchanged)
                && !matches!(theirs, Change::Absent | Change::Unchanged);
            assert_eq!(
                Reason::new(
                    Kind::EntityConflict,
                    entity("f"),
                    Evidence::WeaveActions { ours, theirs }
                )
                .valid(),
                expected
            );
        }
    }
    let mut empty = Reason::new(Kind::LineConflict, Subject::File, Evidence::GitLineConflict);
    empty.evidence.clear();
    assert!(!empty.valid());
    assert!(!Reason::new(
        Kind::EntityConflict,
        Subject::File,
        Evidence::RawBothChanged
    )
    .valid());
    assert!(!Reason::new(Kind::LineConflict, Subject::File, Evidence::LayoutChanged).valid());
    assert!(!Reason::new(
        Kind::EntityConflict,
        Subject::File,
        Evidence::IdenticalEdits
    )
    .valid());
    assert!(!Reason::new(Kind::LineConflict, entity("f"), Evidence::IdenticalEdits).valid());
    for action in [Change::Unchanged, Change::Absent] {
        assert!(!Reason::new(
            Kind::EntityConflict,
            entity("f"),
            Evidence::WeaveActions {
                ours: action,
                theirs: Change::Modified
            }
        )
        .valid());
    }
    assert!(!move_reason(movement(Opposite::Unchanged)).valid());
    let mut candidate = movement(Opposite::Modified);
    candidate.destination_count = 0;
    assert!(!move_reason(candidate).valid());
    let mut invalid = move_reason(movement(Opposite::Modified));
    invalid.subject = Subject::File;
    assert!(!invalid.valid());
    for json in [
        REASON_UNKNOWN_KIND_JSON,
        REASON_UNKNOWN_EVIDENCE_JSON,
        REASON_UNKNOWN_FIELD_JSON,
    ] {
        assert!(serde_json::from_str::<Reason>(json).is_err());
    }
}

#[test]
fn rename_move_evidence_validates_names_counts_and_locations() {
    let mut candidate = movement(Opposite::Modified);
    candidate.target.name = "g".into();
    candidate.matched_by = MoveMatch::NameNormalized {
        grammar: "go".into(),
        entity_type: "function".into(),
        old_name: "f".into(),
        new_name: "g".into(),
        old_occurrences: 2,
        new_occurrences: 2,
        comparison: strict_weave::reason::RenameComparison::Text,
    };
    assert!(candidate.valid());
    let reason = move_reason(candidate.clone());
    assert!(reason.valid());
    let json = serde_json::to_string(&reason).unwrap();
    assert_eq!(serde_json::from_str::<Reason>(&json).unwrap(), reason);
    for mutation in [
        "same_name",
        "empty_grammar",
        "wrong_label",
        "zero",
        "different_count",
        "exact",
        "zero_tokens",
    ] {
        let mut invalid = candidate.clone();
        if let MoveMatch::NameNormalized {
            grammar,
            new_name,
            old_occurrences,
            new_occurrences,
            comparison,
            ..
        } = &mut invalid.matched_by
        {
            match mutation {
                "same_name" => *new_name = "f".into(),
                "empty_grammar" => grammar.clear(),
                "wrong_label" => invalid.target.entity_type = "class".into(),
                "zero" => *old_occurrences = 0,
                "different_count" => *new_occurrences = 1,
                "zero_tokens" => {
                    *comparison = strict_weave::reason::RenameComparison::Syntax { tokens: 0 };
                }
                _ => {}
            }
        }
        if mutation == "exact" {
            invalid.matched_by = MoveMatch::Exact;
        }
        assert!(!invalid.valid(), "{mutation}");
    }
    if let MoveMatch::NameNormalized { comparison, .. } = &mut candidate.matched_by {
        *comparison = strict_weave::reason::RenameComparison::Syntax { tokens: 12 };
    }
    let reason = move_reason(candidate);
    assert!(reason.valid());
    assert_eq!(
        serde_json::from_slice::<Reason>(&serde_json::to_vec(&reason).unwrap()).unwrap(),
        reason
    );
}

#[test]
fn human_output_escapes_entity_and_evidence_control_characters() {
    let r: Reason = serde_json::from_str(REASON_CONTROLS_JSON).unwrap();
    assert!(r
        .lines(true)
        .iter()
        .all(|line| !line.chars().any(char::is_control)));
    let movement = move_reason(movement(Opposite::Unknown));
    assert!(movement.lines(false)[1].contains("另一侧=无法确认"));
    assert!(!movement.lines(false)[1].contains("另一侧=modified"));
}

#[test]
fn provenance_is_explicit_for_every_evidence_and_does_not_claim_independent_checks() {
    use strict_weave::reason::Source;
    let cases = [
        (Evidence::GitLineConflict, Source::Git),
        (Evidence::RawBothChanged, Source::Analyze),
        (Evidence::IdenticalEdits, Source::Analyze),
        (
            Evidence::WeaveActions {
                ours: Change::Deleted,
                theirs: Change::Modified,
            },
            Source::Weave,
        ),
        (
            Evidence::WeaveRefusal {
                refusal: WeaveRefusal::ModifyDelete {
                    modified_in: Side::Theirs,
                },
            },
            Source::Weave,
        ),
        (Evidence::PartitionUnavailable, Source::Analyze),
        (Evidence::WeaveUnavailable, Source::Analyze),
        (Evidence::LayoutChanged, Source::Analyze),
        (
            Evidence::MoveCandidate {
                candidate: Box::new(movement(Opposite::Unknown)),
            },
            Source::Analyze,
        ),
    ];
    for (evidence, source) in &cases {
        assert_eq!(evidence.source(), *source);
        assert!(evidence
            .summary()
            .starts_with(&format!("[{}]：", source.label())));
    }
    let mut reason = Reason::new(
        Kind::EntityConflict,
        entity("f"),
        Evidence::WeaveActions {
            ours: Change::Deleted,
            theirs: Change::Modified,
        },
    );
    reason.evidence.insert(Evidence::WeaveRefusal {
        refusal: WeaveRefusal::ModifyDelete {
            modified_in: Side::Theirs,
        },
    });
    assert_eq!(
        reason.lines(false),
        REASON_PROVENANCE_TXT.lines().collect::<Vec<_>>()
    );
    let unavailable = Reason::new(
        Kind::AnalysisUnavailable,
        Subject::File,
        Evidence::WeaveUnavailable,
    );
    assert_eq!(
        unavailable.lines(false),
        REASON_UNAVAILABLE_TXT.lines().collect::<Vec<_>>()
    );
}
