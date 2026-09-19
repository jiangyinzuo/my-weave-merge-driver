mod common;
use common::{text, Case, STALE_TEXT};
use std::collections::BTreeMap;
use strict_weave::reason::{Kind, Opposite, Side};
use strict_weave::{
    analysis,
    merge::{self, Labels},
};

const BROKEN_JSON: &str = "{broken";

const F: &str = include_str!("fixtures/moves/move-target.go.theirs");

fn snapshot(files: &[(&str, &str)]) -> BTreeMap<String, String> {
    files
        .iter()
        .map(|(path, text)| (path.to_string(), text.to_string()))
        .collect()
}

fn analyze(files: [BTreeMap<String, String>; 3]) -> analysis::Report {
    analysis::analyze(
        &files,
        ["base-tree".into(), "ours-tree".into(), "theirs-tree".into()],
    )
    .unwrap()
}

fn moved() -> [BTreeMap<String, String>; 3] {
    let c = Case::load("moves/move-source.go");
    [
        snapshot(&[("source.go", &c.base)]),
        snapshot(&[("source.go", &c.ours)]),
        snapshot(&[("source.go", &c.theirs), ("target.go", F)]),
    ]
}

#[test]
fn global_modify_move_is_deterministic_and_only_adds_conflicts() {
    let inputs = moved();
    let report = analyze(inputs.clone());
    assert!(report.conflicted());
    assert_eq!(report.move_candidates.len(), 1);
    let candidate = &report.move_candidates[0];
    assert_eq!(candidate.side, Side::Theirs);
    assert_eq!(candidate.base.path, "source.go");
    assert_eq!(candidate.target.path, "target.go");
    assert_eq!(candidate.opposite, Opposite::Modified);
    for path in ["source.go", "target.go"] {
        assert!(report.files[path]
            .reasons
            .iter()
            .any(|r| r.kind == Kind::ModifyVsMove));
    }
    for _ in 0..3 {
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&analyze(inputs.clone())).unwrap()
        );
    }
    // Destination is a clean unilateral addition in the local three-way merge.
    let labels = Labels::default();
    let target = Case::load("moves/move-target.go");
    let mut outcome = target.run();
    assert!(!outcome.conflicted());
    analysis::augment(
        &mut outcome,
        &report.files["target.go"],
        target.texts(),
        &labels,
        7,
    );
    assert!(outcome.conflicted());
    assert!(outcome.content.contains("<<<<<<<") && outcome.content.contains(F));
    let conflict = Case::load("moves/calculate.go");
    let mut existing = conflict.run();
    let before = existing.content.clone();
    analysis::augment(
        &mut existing,
        &report.files["source.go"],
        conflict.texts(),
        &labels,
        7,
    );
    assert_eq!(existing.content, before);
    assert!(existing
        .reasons
        .iter()
        .any(|r| r.kind == Kind::LineConflict));
    let swapped = analyze([inputs[0].clone(), inputs[2].clone(), inputs[1].clone()]);
    assert_eq!(swapped.move_candidates[0].side, Side::Ours);
    assert!(swapped.conflicted());
}

#[test]
fn ambiguous_moves_keep_every_candidate_but_copies_are_not_moves() {
    let mut inputs = moved();
    inputs[2].insert("another.go".into(), F.into());
    let report = analyze(inputs);
    assert_eq!(report.move_candidates.len(), 2);
    assert!(report
        .move_candidates
        .iter()
        .all(|c| c.destination_count == 2));
    let original = snapshot(&[("source.go", F)]);
    let copied = snapshot(&[("source.go", F), ("target.go", F)]);
    let report = analyze([original.clone(), original, copied]);
    assert!(report.move_candidates.is_empty());
    assert!(!report.conflicted());
}

#[test]
fn pure_move_annotates_without_conflict_and_rename_is_not_claimed() {
    let source = snapshot(&[("source.go", F)]);
    let report = analyze([
        source.clone(),
        source.clone(),
        snapshot(&[("target.go", F)]),
    ]);
    assert_eq!(report.move_candidates.len(), 1);
    assert_eq!(report.move_candidates[0].opposite, Opposite::Unchanged);
    assert!(!report.conflicted());
    let deleted = Case::load("moves/move-delete.go");
    let mut local = deleted.run();
    analysis::augment(
        &mut local,
        &report.files["source.go"],
        deleted.texts(),
        &Labels::default(),
        7,
    );
    assert!(!local.conflicted());
    assert_eq!(local.related_moves, report.move_candidates);
    assert_eq!(local.content, deleted.expected());
    for new in [
        text("moves/move-renamed.go", "ours"),
        text("moves/calculate.go", "ours"),
    ] {
        let report = analyze([
            source.clone(),
            source.clone(),
            snapshot(&[("target.go", &new)]),
        ]);
        assert!(report.move_candidates.is_empty());
    }
}

#[test]
fn parser_failure_keeps_strict_conflicts_and_reports_incomplete_analysis() {
    let report = analyze([
        snapshot(&[("a.go", F)]),
        snapshot(&[("a.go", &text("moves/move-invalid.go", "ours"))]),
        snapshot(&[
            ("a.go", &text("moves/move-invalid.go", "theirs")),
            ("target.go", F),
        ]),
    ]);
    assert!(!report.warnings.is_empty());
    assert!(report.conflicted());
    assert_eq!(report.move_candidates[0].opposite, Opposite::Unknown);
}

#[test]
fn immutable_result_rejects_stale_reversed_unknown_and_invalid_inputs() {
    let inputs = moved();
    let report = analyze(inputs.clone());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("analysis.json");
    analysis::save(&path, &report).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(std::fs::metadata(&path).unwrap().permissions().readonly());
    assert!(analysis::save(&path, &report).is_err());
    let texts = inputs.each_ref().map(|s| s["source.go"].as_str());
    assert!(analysis::load_for_driver(&path, "source.go", texts).is_ok());
    assert!(analysis::load_for_driver(&path, "missing.go", texts).is_err());
    assert!(analysis::load_for_driver(&path, "source.go", [texts[0], texts[2], texts[1]]).is_err());
    assert!(
        analysis::load_for_driver(&path, "source.go", [texts[0], STALE_TEXT, texts[2]]).is_err()
    );
    assert_eq!(bytes, std::fs::read(&path).unwrap());
    let bad = dir.path().join("bad.json");
    std::fs::write(&bad, BROKEN_JSON).unwrap();
    assert!(analysis::load_for_driver(&bad, "source.go", texts).is_err());
    let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["engine"] = "strict-weave-global-v4".into();
    std::fs::write(&bad, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(analysis::load_for_driver(&bad, "source.go", texts).is_err());
    json = serde_json::from_slice(&bytes).unwrap();
    json["schema_version"] = 1.into();
    std::fs::write(&bad, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(analysis::load_for_driver(&bad, "source.go", texts).is_err());
}

#[test]
fn global_default_report_does_not_override_zdiff3_display_explanation() {
    let c = Case::load("layout/adjacent.ts");
    let [base, ours, theirs] = c.texts();
    let report = analyze([
        snapshot(&[("a.ts", base)]),
        snapshot(&[("a.ts", ours)]),
        snapshot(&[("a.ts", theirs)]),
    ]);
    let labels = Labels::default();
    let mut outcome = merge::merge_with_style(
        base,
        ours,
        theirs,
        "a.ts",
        &labels,
        7,
        merge::ConflictStyle::Zdiff3,
    )
    .unwrap();
    let content = outcome.content.clone();
    let related_moves = outcome.related_moves.clone();
    let reasons = outcome.reasons.clone();
    analysis::augment(
        &mut outcome,
        &report.files["a.ts"],
        [base, ours, theirs],
        &labels,
        7,
    );
    assert!(outcome.conflicted());
    assert_eq!(outcome.content, content);
    assert_eq!(outcome.related_moves, related_moves);
    assert_eq!(outcome.reasons, reasons);
}

#[test]
fn cached_reason_tree_is_validated_and_repeated_augmentation_is_idempotent() {
    let inputs = moved();
    let report = analyze(inputs.clone());
    assert_eq!(report.schema_version, 2);
    assert!(report
        .files
        .values()
        .all(|f| f.reasons.iter().all(|r| r.valid())));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.json");
    let texts = inputs.each_ref().map(|s| s["source.go"].as_str());
    let json = serde_json::to_value(&report).unwrap();
    for mutation in ["empty", "wrong_parent", "unknown_evidence", "unknown_field"] {
        let mut invalid = json.clone();
        let reason = &mut invalid["files"]["source.go"]["reasons"][0];
        match mutation {
            "empty" => reason["evidence"] = serde_json::json!([]),
            "wrong_parent" => reason["kind"] = serde_json::json!("modify_vs_move"),
            "unknown_evidence" => reason["evidence"][0]["type"] = serde_json::json!("unknown"),
            _ => reason["unexpected"] = true.into(),
        }
        std::fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(
            analysis::load_for_driver(&path, "source.go", texts).is_err(),
            "{mutation}"
        );
    }
    let labels = Labels::default();
    let mut outcome = merge::merge(texts[0], texts[1], texts[2], "source.go", &labels, 7).unwrap();
    analysis::augment(&mut outcome, &report.files["source.go"], texts, &labels, 7);
    let reasons = outcome.reasons.clone();
    let content = outcome.content.clone();
    analysis::augment(&mut outcome, &report.files["source.go"], texts, &labels, 7);
    assert_eq!(outcome.reasons, reasons);
    assert_eq!(outcome.content, content);
}
