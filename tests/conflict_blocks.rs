mod common;

use strict_weave::merge::{conflict_box, Labels};

// Read each side back out of the rendered box, including the context moved
// outside. This checks byte preservation independently of trimming boundaries.
fn restore(output: &str) -> [String; 3] {
    let lines: Vec<_> = output.split_inclusive('\n').collect();
    let marker = |prefix: &str| {
        let found: Vec<_> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.starts_with(prefix))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(found.len(), 1, "exactly one {prefix} marker must survive");
        found[0]
    };
    let (o, b, sep, end) = (
        marker("<<<<<<<"),
        marker("|||||||"),
        marker("======="),
        marker(">>>>>>>"),
    );
    assert!(o < b && b < sep && sep < end);
    let side = |start, stop| {
        [&lines[..o], &lines[start..stop], &lines[end + 1..]]
            .concat()
            .concat()
    };
    [side(b + 1, sep), side(o + 1, b), side(sep + 1, end)]
}

#[test]
fn context_trimming_preserves_all_three_versions() {
    let variants: Vec<_> = (0..8)
        .map(|i| common::Case::load(&format!("git-baseline/git-variant-{i}.ts")))
        .collect();
    for ours in &variants {
        for theirs in &variants {
            let original = [&ours.base, &ours.ours, &theirs.ours];
            let output = conflict_box(
                original[0],
                original[1],
                original[2],
                &Labels::default(),
                7,
                "test",
            );
            assert_eq!(restore(&output), original.map(|s| s.to_owned()));
        }
    }
    for name in [
        "nested/decorated-method.py",
        "nested/method-crlf.py",
        "nested/method-no-final-newline.py",
        "nested/method-identical.py",
    ] {
        let c = common::Case::load(name);
        let output = conflict_box(&c.base, &c.ours, &c.theirs, &Labels::default(), 7, "test");
        assert_eq!(restore(&output), [c.base, c.ours, c.theirs]);
    }
}

#[test]
fn forced_equal_and_empty_conflicts_keep_markers() {
    for text in ["", "same\n", "same\r\n"] {
        let output = conflict_box(text, text, text, &Labels::default(), 7, "external reason");
        assert!(output.starts_with("<<<<<<<"));
        assert_eq!(
            restore(&output),
            [text.to_owned(), text.to_owned(), text.to_owned()]
        );
    }
}

#[test]
fn line_endings_and_boundary_insertions_remain_visible() {
    // Short auxiliary strings exercise terminator/boundary mechanics; complete
    // source examples and expected output remain in text fixtures.
    for texts in [
        ["head\ntail", "head\nadded\ntail", "head\nother\ntail"],
        ["same\n", "same\r\n", "same\n"],
        ["", "one\n", "two\n"],
        ["head\n", "head\none\n", "head\ntwo\n"],
        ["head\ntail\n", "head\n", "head\nother\ntail\n"],
        ["repeat\nrepeat\n", "repeat\n", "repeat\nrepeat\nrepeat\n"],
    ] {
        let output = conflict_box(texts[0], texts[1], texts[2], &Labels::default(), 7, "test");
        assert_eq!(restore(&output), texts.map(str::to_owned));
    }
    // A changed unterminated line needs a separator newline before the marker,
    // as in the original full-entity renderer; it must not join the marker.
    let output = conflict_box("same", "same\n", "other", &Labels::default(), 7, "test");
    assert!(output.starts_with("<<<<<<<"));
    assert_eq!(
        restore(&output),
        ["same\n", "same\n", "other\n"].map(str::to_owned)
    );
}
