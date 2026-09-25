//! Directory fixtures share one immutable global report across all file drivers.
use super::{check_text, common, compare, TextCase};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use strict_weave::analysis;

type Files = BTreeMap<String, PathBuf>;

/// A tracked empty file can represent an empty tree; Git cannot track empty directories.
fn snapshot_files(root: &Path) -> Result<Files, String> {
    if root.is_file() {
        if fs::metadata(root).map_err(|e| e.to_string())?.len() == 0 {
            return Ok(Files::new());
        }
        return Err(format!("{} 必须是目录或表示空快照的空文件", root.display()));
    }
    let mut files = Files::new();
    collect_files(root, root, &mut files)?;
    Ok(files)
}

fn collect_files(root: &Path, directory: &Path, files: &mut Files) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|e| format!("{}: {e}", directory.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            collect_files(root, &path, files)?;
        } else if kind.is_file() {
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .ok_or_else(|| format!("非 UTF-8 fixture 路径：{}", path.display()))?
                .to_owned();
            files.insert(relative, path);
        } else {
            return Err(format!("fixture 必须为普通文件：{}", path.display()));
        }
    }
    Ok(())
}

/// Output/diagnostic directories must cover exactly the input paths, including deleted files.
fn require_paths(root: &Path, paths: &BTreeSet<String>) -> Result<(), String> {
    let actual: BTreeSet<_> = snapshot_files(root)?.into_keys().collect();
    if &actual != paths {
        return Err(format!(
            "{} 路径集合不匹配：缺少 {:?}，多出 {:?}",
            root.display(),
            paths.difference(&actual).collect::<Vec<_>>(),
            actual.difference(paths).collect::<Vec<_>>()
        ));
    }
    Ok(())
}

pub(super) fn check_case(
    directory: &Path,
    name: &str,
    artifacts: &Path,
    zdiff3: bool,
    detailed: bool,
) -> Result<(), String> {
    let file = |suffix: &str| directory.join(format!("{name}.{suffix}"));
    if file("entities").exists() {
        return Err(format!("{name}: .entities 仅支持单文件用例"));
    }
    if file("path").exists() {
        return Err(format!(
            "{name}: 多文件用例直接使用快照内相对路径，不支持 .path"
        ));
    }
    let inputs = [
        snapshot_files(&file("base"))?,
        snapshot_files(&file("ours"))?,
        snapshot_files(&file("theirs"))?,
    ];
    let paths: BTreeSet<String> = inputs.iter().flat_map(|s| s.keys().cloned()).collect();
    if paths.is_empty() {
        return Err(format!("{name}: 多文件用例没有源码文件"));
    }
    require_paths(&file("output"), &paths)?;
    for suffix in ["output-zdiff3", "stderr", "stderr-zdiff3", "stderr-details"] {
        if file(suffix).exists() {
            require_paths(&file(suffix), &paths)?;
        }
    }
    if file("exit").exists() {
        for path in snapshot_files(&file("exit"))?.keys() {
            if !paths.contains(path) {
                return Err(format!("{name}.exit 包含未知路径：{path}"));
            }
        }
    }
    let mut snapshots = [BTreeMap::new(), BTreeMap::new(), BTreeMap::new()];
    for (snapshot, files) in snapshots.iter_mut().zip(&inputs) {
        for (path, file) in files {
            snapshot.insert(
                path.clone(),
                fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?,
            );
        }
    }
    let report = analysis::analyze(
        &snapshots,
        [
            "fixture:base".into(),
            "fixture:ours".into(),
            "fixture:theirs".into(),
        ],
    )
    .map_err(|e| format!("{name}: {e:#}"))?;
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let report_path = scratch.path().join("analysis.json");
    analysis::save(&report_path, &report).map_err(|e| format!("{e:#}"))?;
    let report_bytes = fs::read(&report_path).map_err(|e| e.to_string())?;
    let mode = format!(
        "{}{}",
        if zdiff3 { ".zdiff3" } else { "" },
        if detailed { ".details" } else { "" }
    );
    let mut errors = Vec::new();
    if let Err(e) = compare(
        &file("analysis"),
        &report_bytes,
        &artifacts.join(format!("{name}{mode}.actual-analysis")),
    ) {
        errors.push(e);
    }
    let options = common::Options::load(name)?;
    let stderr = if detailed {
        "stderr-details"
    } else if zdiff3 && file("stderr-zdiff3").exists() {
        "stderr-zdiff3"
    } else {
        "stderr"
    };
    for path in paths {
        let case = TextCase {
            inputs: inputs
                .each_ref()
                .map(|snapshot| snapshot.get(&path).cloned()),
            default_output: file("output").join(&path),
            output: file(if zdiff3 { "output-zdiff3" } else { "output" }).join(&path),
            stderr: file(stderr).join(&path),
            exit: file("exit").join(&path),
            path: path.clone(),
        };
        // Unchanged paths are intentionally omitted by global analysis.
        // Changed paths must always consume the shared report.
        let changed = snapshots[0].get(&path) != snapshots[1].get(&path)
            || snapshots[0].get(&path) != snapshots[2].get(&path);
        let analysis = changed.then_some(report_path.as_path());
        if let Err(e) = check_text(
            &case,
            &options,
            &artifacts.join(format!("{name}{mode}")).join(&path),
            zdiff3,
            detailed,
            analysis,
        ) {
            errors.push(e);
        }
    }
    if fs::read(&report_path).map_err(|e| e.to_string())? != report_bytes {
        errors.push(format!("{name}: driver 修改了共享分析结果"));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_empty_snapshots_are_distinct_from_present_empty_files() {
        let temp = tempfile::tempdir().unwrap();
        let absent = temp.path().join("missing");
        assert!(snapshot_files(&absent).is_err());
        let empty = temp.path().join("empty.base");
        fs::write(&empty, b"").unwrap();
        assert!(snapshot_files(&empty).unwrap().is_empty());
        let directory = temp.path().join("present.base");
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("empty.go"), b"").unwrap();
        let files = snapshot_files(&directory).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files.contains_key("empty.go"));
        fs::write(&empty, b"not an empty snapshot").unwrap();
        assert!(snapshot_files(&empty).is_err());
    }

    #[test]
    fn missing_or_extra_output_paths_fail_instead_of_skipping_files() {
        let temp = tempfile::tempdir().unwrap();
        let paths = BTreeSet::from(["source.go".into(), "target.go".into()]);
        fs::write(temp.path().join("source.go"), b"").unwrap();
        assert!(require_paths(temp.path(), &paths).is_err());
        fs::write(temp.path().join("target.go"), b"").unwrap();
        assert!(require_paths(temp.path(), &paths).is_ok());
        fs::write(temp.path().join("extra.go"), b"").unwrap();
        assert!(require_paths(temp.path(), &paths).is_err());
    }
}
