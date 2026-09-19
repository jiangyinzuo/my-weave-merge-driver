//! Driver-only workflows remain usable without wrapper state or reports.
use crate::support::*;
use std::fs;

#[test]
fn native_merge_rebase_cherry_pick_and_stash_pop_invoke_the_driver() {
    for operation in ["merge", "rebase", "cherry-pick", "stash"] {
        let r = Repo::new(SIMPLE, "calc.go");
        let driver = format!("'{}' driver %O %A %B %P %L", TOOL.replace('\'', "'\\''"));
        r.git(&["config", "merge.strict-weave.driver", &driver]);
        let args = if operation == "stash" {
            r.copy(SIMPLE, "theirs", "calc.go");
            r.git(&["stash", "push", "-q"]);
            r.copy(SIMPLE, "ours", "calc.go");
            r.commit("ours");
            vec!["stash", "pop"]
        } else {
            r.diverge(SIMPLE, "calc.go");
            vec![operation, "other"]
        };
        let original = r.head();
        let out = r.command("git", &args).output().unwrap();
        assert_code(&out, 1);
        assert!(stderr(&out).contains("ENTITY_CONFLICT"), "{}", stderr(&out));
        assert!(!r.git(&["ls-files", "-u"]).is_empty());
        let result = fs::read_to_string(r.path().join("calc.go")).unwrap();
        assert!(
            result.contains("<<<<<<<") && result.contains("|||||||") && result.contains(">>>>>>>")
        );
        if operation == "stash" {
            assert!(!r.git(&["stash", "list"]).is_empty());
            r.git(&["reset", "--hard", &original]);
        } else {
            r.git(&[operation, "--abort"]);
        }
        assert_eq!(r.head(), original);
        r.assert_file("calc.go", SIMPLE, "ours");
        r.assert_clean();
    }
}
