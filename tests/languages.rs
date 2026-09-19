//! Coverage follows upstream's compiled language registry, not a copied list.
mod common;
use common::Case;
use sem_core::parser::plugins::code::languages::{get_all_code_extensions, get_language_config};
use std::collections::{BTreeMap, BTreeSet};
use strict_weave::{
    analysis,
    merge::{merge, Labels},
    reason::{Evidence, Kind, Opposite},
};

fn supported_code_extensions() -> Vec<String> {
    weave_core::supported_merge_extensions()
        .into_iter()
        .filter(|ext| get_all_code_extensions().contains(&ext.as_str()))
        .collect()
}

#[test]
fn every_upstream_code_extension_uses_entity_analysis() {
    let extensions = supported_code_extensions();
    assert!(!extensions.is_empty());
    let mut grammars = BTreeSet::new();
    let extension_count = extensions.len();
    for ext in extensions {
        let config = get_language_config(&ext).unwrap();
        grammars.insert(config.id);
        // Aliases share the canonical fixture's exact bytes, including .mts,
        // .cts, .mjs, .cjs and case-sensitive paths as resolved by upstream.
        let c = Case::load(&format!("languages/language{}", config.extensions[0]));
        let path = format!("sample{ext}");
        let out = merge(
            &c.base,
            &c.ours,
            &c.theirs,
            &path,
            &common::Options::default().labels(),
            7,
        )
        .unwrap();
        assert_eq!(out.content, c.expected(), "{path}");
        assert!(
            !out.reasons
                .iter()
                .any(|r| r.kind == Kind::AnalysisUnavailable),
            "{path}: {:?}",
            out.reasons
        );
        assert!(
            out.reasons.iter().any(|r| r.kind == Kind::EntityConflict
                && r.evidence
                    .iter()
                    .any(|e| matches!(e, Evidence::WeaveActions { .. }))),
            "{path}: {:?}",
            out.reasons
        );
        for (ours, theirs) in [(&c.ours, &c.base), (&c.base, &c.ours)] {
            let out = merge(&c.base, ours, theirs, &path, &Labels::default(), 7).unwrap();
            assert!(!out.conflicted(), "{path}: {:?}", out.reasons);
            assert_eq!(out.content, c.ours, "{path}");
        }
    }
    eprintln!(
        "covered {} code grammars and {extension_count} extensions",
        grammars.len()
    );
}

#[test]
fn every_upstream_code_language_participates_in_global_move_analysis() {
    let mut checked = BTreeSet::new();
    for ext in supported_code_extensions() {
        let config = get_language_config(&ext).unwrap();
        if !checked.insert(config.id) {
            continue;
        }
        let c = Case::load(&format!("languages/language{}", config.extensions[0]));
        let source = format!("old{ext}");
        let target = format!("new{ext}");
        let files = [
            BTreeMap::from([(source.clone(), c.base.clone())]),
            BTreeMap::from([(source.clone(), c.ours.clone())]),
            BTreeMap::from([(target.clone(), c.base.clone())]),
        ];
        let report =
            analysis::analyze(&files, ["base".into(), "ours".into(), "theirs".into()]).unwrap();
        assert!(report.warnings.is_empty(), "{ext}: {:?}", report.warnings);
        assert_eq!(
            report.move_candidates.len(),
            1,
            "{ext}: {:?}",
            report.move_candidates
        );
        assert_eq!(
            report.move_candidates[0].opposite,
            Opposite::Modified,
            "{ext}"
        );
        for path in [&source, &target] {
            assert!(
                report.files[path]
                    .reasons
                    .iter()
                    .any(|r| r.kind == Kind::ModifyVsMove),
                "{path}"
            );
        }
    }
}
