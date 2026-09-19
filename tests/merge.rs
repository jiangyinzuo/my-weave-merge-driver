mod common;
use common::Case;
use strict_weave::merge::{merge, merge_with_style, ConflictStyle, Labels};
use strict_weave::reason::{Change, Evidence, Kind, Side, Subject, WeaveRefusal};

// Text outcomes are asserted by the automatically discovered fixture suite.
// These tests assert typed evidence and properties over the same fixture inputs.
#[test]
fn strict_policy_reuses_upstream_classification() {
    for name in ["entities/disjoint.go", "entities/identical.go"] {
        let c = Case::load(name);
        assert!(weave_core::entity_merge(&c.base, &c.ours, &c.theirs, &c.path).is_clean());
        let out = c.run();
        assert_eq!(out.reasons.len(), 1);
        assert_eq!(out.reasons[0].kind, Kind::EntityConflict);
        assert_eq!(
            out.reasons[0].subject,
            Subject::Entity {
                entity_type: "function".into(),
                name: "a".into()
            }
        );
        assert_eq!(
            out.reasons[0].evidence,
            [Evidence::WeaveActions {
                ours: Change::Modified,
                theirs: Change::Modified
            }]
            .into()
        );
    }
}

#[test]
fn modify_delete_reuses_upstream_refusal() {
    let out = Case::load("entities/delete.go").run();
    assert!(out.reasons.iter().all(|r| r
        .evidence
        .iter()
        .all(|e| !matches!(e, Evidence::WeaveActions { .. } | Evidence::RawBothChanged))));
    assert!(out.reasons.iter().any(|r| r.kind == Kind::LineConflict));
    assert!(out
        .reasons
        .iter()
        .any(|r| r.evidence.contains(&Evidence::WeaveRefusal {
            refusal: WeaveRefusal::ModifyDelete {
                modified_in: Side::Theirs
            }
        })));
}

#[test]
fn strict_policy_reuses_weave_classification_per_entity() {
    let out = Case::load("entities/two-entities.go").run();
    assert_eq!(out.reasons.len(), 3);
    assert_eq!(
        out.reasons
            .iter()
            .filter(|r| r.kind == Kind::LineConflict)
            .count(),
        1
    );
    for name in ["a", "b"] {
        let reason = out
            .reasons
            .iter()
            .find(|r| {
                r.subject
                    == Subject::Entity {
                        entity_type: "function".into(),
                        name: name.into(),
                    }
            })
            .unwrap();
        assert_eq!(reason.kind, Kind::EntityConflict);
        assert_eq!(
            reason.evidence,
            [Evidence::WeaveActions {
                ours: Change::Modified,
                theirs: Change::Modified
            }]
            .into()
        );
    }
}

#[test]
fn upstream_refusal_does_not_skip_strict_policy_for_another_entity() {
    let c = Case::load("entities/mixed-rules.go");
    for (ours, theirs) in [(&c.ours, &c.theirs), (&c.theirs, &c.ours)] {
        let upstream = weave_core::entity_merge(&c.base, ours, theirs, &c.path);
        assert_eq!(upstream.conflicts.len(), 1);
        assert_eq!(upstream.conflicts[0].entity_name, "a");
        let out = merge(&c.base, ours, theirs, &c.path, &Labels::default(), 7).unwrap();
        for (name, evidence) in [
            (
                "a",
                Evidence::WeaveRefusal {
                    refusal: WeaveRefusal::BothModified,
                },
            ),
            (
                "b",
                Evidence::WeaveActions {
                    ours: Change::Modified,
                    theirs: Change::Modified,
                },
            ),
        ] {
            let reason = out
                .reasons
                .iter()
                .find(|r| {
                    r.subject
                        == Subject::Entity {
                            entity_type: "function".into(),
                            name: name.into(),
                        }
                })
                .unwrap();
            assert_eq!(reason.evidence, [evidence].into());
        }
    }
}

#[test]
fn raw_supplement_catches_name_normalization_omitted_by_weave() {
    for name in [
        "entities/normalized-identical.go",
        "entities/normalized-edit.go",
    ] {
        let c = Case::load(name);
        for (ours, theirs) in [(&c.ours, &c.theirs), (&c.theirs, &c.ours)] {
            let analysis = weave_core::v2::analyze_default(&c.base, ours, theirs, &c.path).unwrap();
            assert!(analysis.iter().all(|(_, cell)| {
                let (ours, theirs) = cell.actions();
                !Change::from(ours).changed() || !Change::from(theirs).changed()
            }));
            let out = merge(&c.base, ours, theirs, &c.path, &Labels::default(), 7).unwrap();
            let reason = out
                .reasons
                .iter()
                .find(|r| r.kind == Kind::EntityConflict)
                .unwrap();
            assert_eq!(reason.evidence, [Evidence::RawBothChanged].into());
        }
    }
}

#[test]
fn unsupported_and_invalid_syntax_refuse_when_both_changed() {
    for name in ["fallback/a", "fallback/invalid-syntax.go"] {
        let out = Case::load(name).run();
        assert!(out
            .reasons
            .iter()
            .any(|r| r.kind == Kind::AnalysisUnavailable));
    }
}

#[test]
fn deterministic_and_side_symmetric() {
    let c = Case::load("entities/nearby.go");
    let first = c.run();
    for _ in 0..4 {
        let out = c.run();
        assert_eq!(first.content, out.content);
        assert_eq!(first.reasons, out.reasons);
    }
    assert!(
        merge(&c.base, &c.theirs, &c.ours, &c.path, &Labels::default(), 7)
            .unwrap()
            .conflicted()
    );
}

#[test]
fn plain_git_conflicts_are_a_subset_of_strict_conflicts() {
    // Read the eight checked-in variants, retaining the full 8x8 Git oracle.
    let variants: Vec<_> = (0..8)
        .map(|i| Case::load(&format!("git-baseline/git-variant-{i}.ts")))
        .collect();
    let base = &variants[0].base;
    let scratch = tempfile::tempdir().unwrap();
    let b = scratch.path().join("base");
    let o = scratch.path().join("ours");
    let t = scratch.path().join("theirs");
    std::fs::write(&b, base).unwrap();
    let mut git_conflicts = 0;
    for (oi, ours) in variants.iter().enumerate() {
        assert_eq!(&ours.base, base);
        for (ti, theirs) in variants.iter().enumerate() {
            std::fs::write(&o, &ours.ours).unwrap();
            std::fs::write(&t, &theirs.ours).unwrap();
            let baseline = std::process::Command::new("git")
                .args(["-c", "merge.conflictStyle=merge", "merge-file", "-p"])
                .args([&o, &b, &t])
                .output()
                .unwrap();
            let code = baseline.status.code().unwrap();
            assert!((0..=127).contains(&code));
            // Independently test the Git property used by our single-run
            // implementation, so entity checks cannot hide a line regression.
            let styles = ["--diff3", "--zdiff3"].map(|style| {
                let output = std::process::Command::new("git")
                    .args(["-c", "merge.conflictStyle=merge", "merge-file", "-p", style])
                    .args([&o, &b, &t])
                    .output()
                    .unwrap();
                let styled_code = output.status.code().unwrap();
                assert!((0..=127).contains(&styled_code));
                assert!(
                    code == 0 || styled_code != 0,
                    "{style} lost plain conflict for {oi}/{ti}"
                );
                styled_code != 0
            });
            assert_eq!(
                styles[0], styles[1],
                "Git style verdict differs for {oi}/{ti}"
            );
            let out = merge(
                base,
                &ours.ours,
                &theirs.ours,
                &ours.path,
                &Labels::default(),
                7,
            )
            .unwrap();
            let compact = merge_with_style(
                base,
                &ours.ours,
                &theirs.ours,
                &ours.path,
                &Labels::default(),
                7,
                ConflictStyle::Zdiff3,
            )
            .unwrap();
            assert_eq!(
                out.conflicted(),
                compact.conflicted(),
                "style changed verdict for {oi}/{ti}"
            );
            assert_eq!(
                out.reasons, compact.reasons,
                "style changed evidence for {oi}/{ti}"
            );
            assert!(out.reasons.iter().all(|r| r.valid()));
            for (styled_conflict, result) in styles.into_iter().zip([&out, &compact]) {
                if styled_conflict {
                    assert!(
                        result.reasons.iter().any(|r| r.kind == Kind::LineConflict),
                        "lost Git line evidence for {oi}/{ti}"
                    );
                }
            }
            if code != 0 {
                git_conflicts += 1;
                assert!(out.conflicted(), "missed plain Git conflict for {oi}/{ti}");
                assert!(out.content.contains("<<<<<<<"));
                assert!(compact.content.contains("<<<<<<<"));
            }
        }
    }
    assert!(git_conflicts > 0);
}

#[test]
fn zdiff3_preserves_full_strict_entity_conflicts() {
    for name in [
        "entities/identical.go",
        "entities/nearby.go",
        "layout/both-added.go",
    ] {
        let c = Case::load(name);
        let default = merge(&c.base, &c.ours, &c.theirs, &c.path, &Labels::default(), 11).unwrap();
        let compact = merge_with_style(
            &c.base,
            &c.ours,
            &c.theirs,
            &c.path,
            &Labels::default(),
            11,
            ConflictStyle::Zdiff3,
        )
        .unwrap();
        assert!(compact.conflicted());
        assert_eq!(compact.content, default.content);
        assert_eq!(compact.reasons, default.reasons);
    }
}
