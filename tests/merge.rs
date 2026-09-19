use strict_weave::merge::{merge, merge_with_style, ConflictStyle, Labels};

fn run(base: &str, ours: &str, theirs: &str, path: &str) -> strict_weave::merge::Outcome {
    merge(base, ours, theirs, path, &Labels::default(), 7).unwrap()
}

const BASE: &str = "func a() int {\n    x := 1\n    y := 2\n    z := 3\n    return y\n}\n";

#[test]
fn disjoint_edits_in_same_function_conflict_and_preserve_three_versions() {
    let ours = BASE.replace("    z := 3\n", "");
    let theirs = BASE.replace("    x := 1\n", "");
    let out = run(BASE, &ours, &theirs, "calc.go");
    assert!(out.conflicted());
    assert!(out.content.contains(BASE));
    assert!(out.content.contains(&ours));
    assert!(out.content.contains(&theirs));
    assert!(out
        .reasons
        .iter()
        .any(|r| r.contains("ENTITY_BOTH_CHANGED")));
}

#[test]
fn identical_edits_are_not_clean() {
    let edited = BASE.replace("y := 2", "y := 5");
    let out = run(BASE, &edited, &edited, "calc.go");
    assert!(out.conflicted());
    assert_eq!(out.content.matches("y := 5").count(), 2);
    assert!(out.content.contains("||||||| base:"));
}

#[test]
fn complete_entity_is_scoped_and_other_entities_merge() {
    let extra = "\nfunc b() int {\n    return 10\n}\n";
    let base = format!("{BASE}{extra}");
    let ours = base.replace("y := 2", "y := 5");
    let theirs = base
        .replace("y := 2", "y := 5")
        .replace("return 10", "return 20");
    let out = run(&base, &ours, &theirs, "calc.go");
    assert!(out.conflicted());
    assert_eq!(out.content.matches("func a()").count(), 3);
    assert_eq!(out.content.matches("func b()").count(), 1);
    assert!(out
        .content
        .ends_with("\nfunc b() int {\n    return 20\n}\n"));
}

#[test]
fn independent_functions_merge_without_source_changes() {
    let base = "export function a() {\n  return 1;\n}\n\nexport function b() {\n  return 2;\n}\n";
    let ours = base.replace("return 1", "return 3");
    let theirs = base.replace("return 2", "return 4");
    let out = run(base, &ours, &theirs, "a.ts");
    assert!(!out.conflicted(), "{:?}", out.reasons);
    assert_eq!(
        out.content,
        base.replace("return 1", "return 3")
            .replace("return 2", "return 4")
    );
}

#[test]
fn adjacent_additions_preserve_line_conflict() {
    let base = "export function existing() { return 0; }\n";
    let ours = format!("{base}\nexport function alpha() {{ return 1; }}\n");
    let theirs = format!("{base}\nexport function beta() {{ return 2; }}\n");
    let out = run(base, &ours, &theirs, "helpers.ts");
    assert!(out.conflicted());
    assert!(out.reasons.iter().any(|r| r.starts_with("LINE_CONFLICT")));
    assert!(out.content.contains(&ours) && out.content.contains(&theirs));
}

#[test]
fn modify_delete_keeps_empty_side_and_base() {
    let edited = BASE.replace("y := 2", "y := 5");
    let out = run(BASE, "", &edited, "calc.go");
    assert!(out.conflicted());
    let lines: Vec<_> = out.content.lines().collect();
    assert!(lines[0].starts_with("<<<<<<<"));
    assert!(lines[1].starts_with("|||||||"));
    assert!(out.content.contains(BASE) && out.content.contains(&edited));
}

#[test]
fn unsupported_and_invalid_syntax_refuse_when_both_changed() {
    let out = run("a\nb\nc\n", "A\nb\nc\n", "a\nb\nC\n", "file.unknown");
    assert!(out
        .reasons
        .iter()
        .any(|r| r.starts_with("ENTITY_ANALYSIS_UNAVAILABLE")));
    let out = run(
        "func a() {\n",
        "func a() {\nx\n",
        "func a() {\ny\n",
        "bad.go",
    );
    assert!(out
        .reasons
        .iter()
        .any(|r| r.starts_with("ENTITY_ANALYSIS_UNAVAILABLE")));
}

#[test]
fn unsupported_one_side_change_is_preserved_exactly() {
    let out = run("old", "new without newline", "old", "file.unknown");
    assert!(!out.conflicted());
    assert_eq!(out.content, "new without newline");
}

#[test]
fn unchanged_file_and_unilateral_go_change_are_clean() {
    assert!(!run(BASE, BASE, BASE, "a.go").conflicted());
    let ours = BASE.replace("y := 2", "y := 5");
    let out = run(BASE, &ours, BASE, "a.go");
    assert!(!out.conflicted());
    assert_eq!(out.content, ours);
}

#[test]
fn tsx_and_class_containers_are_conservative() {
    let base = "export function View() {\n  return <div>old</div>;\n}\n";
    let edit = base.replace("old", "new");
    assert!(run(base, &edit, &edit, "view.tsx").conflicted());
    let base = "class C {\n  a() { return 1; }\n  b() { return 2; }\n}\n";
    let out = run(
        base,
        &base.replace("return 1", "return 3"),
        &base.replace("return 2", "return 4"),
        "class.ts",
    );
    assert!(out.conflicted());
}

#[test]
fn duplicate_names_and_line_endings_never_escape_strict_check() {
    let base = "func a() int { return 1 }\nfunc a() int { return 2 }\n";
    assert!(run(
        base,
        &base.replace("return 1", "return 3"),
        &base.replace("return 2", "return 4"),
        "a.go"
    )
    .conflicted());
    let base = BASE.replace('\n', "\r\n");
    let edit = base.replace("y := 2", "y := 5");
    let out = run(&base, &edit, &edit, "a.go");
    assert!(out.conflicted());
    assert!(out.content.contains(&base));
    assert_eq!(out.content.matches(&edit).count(), 2);
}

#[test]
fn layout_and_nonentity_changes_are_conservative() {
    let base = "func a() {}\nfunc b() {}\n";
    let ours = "func a() { println(1) }\nfunc b() {}\n";
    let theirs = "func b() {}\nfunc a() {}\n";
    assert!(run(base, ours, theirs, "a.go").conflicted());
    let base = "// one\n// two\n// three\n\nfunc a() {}\n";
    let out = run(
        base,
        &base.replace("// one", "// ONE"),
        &base.replace("// three", "// THREE"),
        "a.go",
    );
    assert!(out.conflicted());
}

#[test]
fn deterministic_and_side_symmetric() {
    let ours = BASE.replace("y := 2", "y := 5");
    let theirs = BASE.replace("z := 3", "z := 8");
    let first = run(BASE, &ours, &theirs, "a.go");
    for _ in 0..4 {
        let out = run(BASE, &ours, &theirs, "a.go");
        assert_eq!(first.content, out.content);
        assert_eq!(first.reasons, out.reasons);
    }
    assert!(run(BASE, &theirs, &ours, "a.go").conflicted());
}

#[test]
fn custom_marker_size_labels_and_invalid_inputs() {
    let edited = BASE.replace("y := 2", "y := 5");
    let labels = Labels {
        ours: "feature/ours\nINJECT".into(),
        base: "123".into(),
        theirs: "abc cherry-pick".into(),
    };
    let out = merge(BASE, &edited, &edited, "a.go", &labels, 11).unwrap();
    assert!(out
        .content
        .starts_with("<<<<<<<<<<< ours: feature/ours INJECT"));
    assert!(out.content.contains("||||||||||| base: 123"));
    assert!(merge(BASE, BASE, BASE, "a.go", &labels, 0).is_err());
    assert!(merge(BASE, "a\0b", BASE, "a.go", &labels, 7).is_err());
    assert!(merge(BASE, "<<<<<<< ours\n", BASE, "a.go", &labels, 7).is_err());
}

#[test]
fn plain_git_conflicts_are_a_subset_of_strict_conflicts() {
    // Compare with the actual plain Git driver, independently from our diff3
    // baseline. This includes deletions, additions, edits and reordering.
    let base = "export function a() {\n  return 1;\n}\n\nexport function b() {\n  return 2;\n}\n";
    let variants = [
        base.to_owned(),
        base.replace("return 1", "return 3"),
        base.replace("return 1", "return 4"),
        base.replace("return 2", "return 5"),
        "export function b() {\n  return 2;\n}\n".into(),
        format!("{base}\nexport function c() {{ return 3; }}\n"),
        format!("{base}\nexport function d() {{ return 4; }}\n"),
        "export function b() {\n  return 2;\n}\n\nexport function a() {\n  return 1;\n}\n".into(),
    ];
    let scratch = tempfile::tempdir().unwrap();
    let b = scratch.path().join("base");
    let o = scratch.path().join("ours");
    let t = scratch.path().join("theirs");
    std::fs::write(&b, base).unwrap();
    let mut git_conflicts = 0;
    for (oi, ours) in variants.iter().enumerate() {
        for (ti, theirs) in variants.iter().enumerate() {
            std::fs::write(&o, ours).unwrap();
            std::fs::write(&t, theirs).unwrap();
            let baseline = std::process::Command::new("git")
                .args(["-c", "merge.conflictStyle=merge", "merge-file", "-p"])
                .args([&o, &b, &t])
                .output()
                .unwrap();
            let code = baseline.status.code().unwrap();
            assert!((0..=127).contains(&code));
            let out = run(base, ours, theirs, "example.ts");
            let compact = merge_with_style(
                base,
                ours,
                theirs,
                "example.ts",
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
    let edited = BASE.replace("y := 2", "y := 5");
    let disjoint = BASE.replace("z := 3", "z := 8");
    let added = format!("{BASE}\nfunc b() int {{ return 1 }}\n");
    let other_added = added.replace("return 1", "return 2");
    for (ours, theirs) in [
        (&edited, &edited),
        (&edited, &disjoint),
        (&added, &other_added),
    ] {
        let default = merge(BASE, ours, theirs, "a.go", &Labels::default(), 11).unwrap();
        let compact = merge_with_style(
            BASE,
            ours,
            theirs,
            "a.go",
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
