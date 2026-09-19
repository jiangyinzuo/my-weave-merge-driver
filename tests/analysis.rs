use std::collections::BTreeMap;
use strict_weave::{
    analysis,
    merge::{self, Labels},
};

const F: &str = "func calculate() int {\n    return 1\n}\n";
const KEEP: &str = "\nfunc retained() int {\n    return 10\n}\n";

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
    let base = format!("{F}{KEEP}");
    let ours = base.replace("return 1\n", "return 2\n");
    [
        snapshot(&[("source.go", &base)]),
        snapshot(&[("source.go", &ours)]),
        snapshot(&[("source.go", KEEP), ("target.go", F)]),
    ]
}

#[test]
fn global_modify_move_is_deterministic_and_only_adds_conflicts() {
    let inputs = moved();
    let report = analyze(inputs.clone());
    assert!(report.conflicted());
    assert_eq!(report.move_candidates.len(), 1);
    let candidate = &report.move_candidates[0];
    assert_eq!(candidate.side, "theirs");
    assert_eq!(candidate.base.path, "source.go");
    assert_eq!(candidate.target.path, "target.go");
    assert!(candidate.opposite_changed);
    for path in ["source.go", "target.go"] {
        assert!(report.files[path]
            .reasons
            .iter()
            .any(|r| r.starts_with("GLOBAL_MODIFY_VS_MOVE")));
    }
    for _ in 0..3 {
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&analyze(inputs.clone())).unwrap()
        );
    }
    // Destination is a clean unilateral addition in the local three-way merge.
    let labels = Labels::default();
    let mut outcome = merge::merge("", "", F, "target.go", &labels, 7).unwrap();
    assert!(!outcome.conflicted());
    analysis::augment(
        &mut outcome,
        &report.files["target.go"],
        ["", "", F],
        &labels,
        7,
    );
    assert!(outcome.conflicted());
    assert!(outcome.content.contains("<<<<<<<") && outcome.content.contains(F));
    let mut existing = merge::merge(
        F,
        &F.replace('1', "2"),
        &F.replace('1', "3"),
        "source.go",
        &labels,
        7,
    )
    .unwrap();
    let before = existing.content.clone();
    analysis::augment(
        &mut existing,
        &report.files["source.go"],
        [F, F, F],
        &labels,
        7,
    );
    assert_eq!(existing.content, before);
    assert!(existing
        .reasons
        .iter()
        .any(|r| r.starts_with("LINE_CONFLICT")));
    let swapped = analyze([inputs[0].clone(), inputs[2].clone(), inputs[1].clone()]);
    assert_eq!(swapped.move_candidates[0].side, "ours");
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
    assert!(!report.move_candidates[0].opposite_changed);
    assert!(!report.conflicted());
    for new in [F.replace("calculate", "renamed"), F.replace('1', "2")] {
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
        snapshot(&[("a.go", "func calculate(\n")]),
        snapshot(&[("a.go", ""), ("target.go", F)]),
    ]);
    assert!(!report.warnings.is_empty());
    assert!(report.conflicted());
    assert!(report.move_candidates[0].opposite_changed);
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
    assert!(analysis::load_for_driver(&path, "source.go", [texts[0], "stale", texts[2]]).is_err());
    assert_eq!(bytes, std::fs::read(&path).unwrap());
    let bad = dir.path().join("bad.json");
    std::fs::write(&bad, "{broken").unwrap();
    assert!(analysis::load_for_driver(&bad, "source.go", texts).is_err());
    let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["schema_version"] = 99.into();
    std::fs::write(&bad, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(analysis::load_for_driver(&bad, "source.go", texts).is_err());
}

#[test]
fn global_default_report_does_not_override_zdiff3_display_explanation() {
    let base = "export function existing() { return 0; }\n";
    let ours = format!("{base}\nexport function alpha() {{ return 1; }}\n");
    let theirs = format!("{base}\nexport function beta() {{ return 2; }}\n");
    let report = analyze([
        snapshot(&[("a.ts", base)]),
        snapshot(&[("a.ts", &ours)]),
        snapshot(&[("a.ts", &theirs)]),
    ]);
    let labels = Labels::default();
    let mut outcome = merge::merge_with_style(
        base,
        &ours,
        &theirs,
        "a.ts",
        &labels,
        7,
        merge::ConflictStyle::Zdiff3,
    )
    .unwrap();
    let content = outcome.content.clone();
    let reasons = outcome.reasons.clone();
    analysis::augment(
        &mut outcome,
        &report.files["a.ts"],
        [base, &ours, &theirs],
        &labels,
        7,
    );
    assert!(outcome.conflicted());
    assert_eq!(outcome.content, content);
    assert_eq!(outcome.reasons, reasons);
}
