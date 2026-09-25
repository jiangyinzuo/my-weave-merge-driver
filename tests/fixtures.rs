//! Recursive text-only test cases: NAME.base / .ours / .theirs / .output.
//! Optional NAME.path, .exit and .stderr specify path, exit status and report.
//! NAME.output-zdiff3 adds a second run with --zdiff3 and the same exit status.
//! NAME.stderr-details verifies --explain-reasons in every available style;
//! output bytes and exit status must stay identical to that style's baseline.
//! NAME.options optionally overrides marker_size and the three labels via JSON.
//! NAME.entities checks base's upstream [type, name] pairs, including children.
//! Directory-valued inputs/output form a multi-file case with NAME.analysis.
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
mod common;
#[path = "fixture_support/multi.rs"]
mod multi;

fn compare(expected_path: &Path, actual: &[u8], actual_path: &Path) -> Result<(), String> {
    let expected =
        fs::read(expected_path).map_err(|e| format!("{}: {e}", expected_path.display()))?;
    if expected == actual {
        return Ok(());
    }
    fs::create_dir_all(actual_path.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(actual_path, actual).map_err(|e| e.to_string())?;
    let diff = Command::new("git")
        .args([
            "diff",
            "--no-index",
            "--no-ext-diff",
            "--no-textconv",
            "--color=never",
            "--",
        ])
        .args([expected_path, actual_path])
        .output()
        .map_err(|e| e.to_string())?;
    let first = expected
        .iter()
        .zip(actual)
        .position(|(a, b)| a != b)
        .unwrap_or(expected.len().min(actual.len()));
    Err(format!(
        "{}：byte {first} 起不同（expected {} bytes，actual {} bytes）\n{}\nactual: {}",
        expected_path.display(),
        expected.len(),
        actual.len(),
        String::from_utf8_lossy(&diff.stdout),
        actual_path.display()
    ))
}

fn check_case(
    directory: &Path,
    name: &str,
    artifacts: &Path,
    zdiff3: bool,
    detailed: bool,
) -> Result<(), String> {
    let file = |suffix: &str| directory.join(format!("{name}.{suffix}"));
    let options = common::Options::load(name)?;
    let output_suffix = if zdiff3 { "output-zdiff3" } else { "output" };
    let stderr_suffix = if detailed {
        "stderr-details"
    } else if zdiff3 && file("stderr-zdiff3").exists() {
        "stderr-zdiff3"
    } else {
        "stderr"
    };
    let mut artifact_name = if zdiff3 {
        format!("{name}.zdiff3")
    } else {
        name.to_owned()
    };
    if detailed {
        artifact_name.push_str(".details");
    }
    let inputs = ["base", "ours", "theirs"].map(&file);
    let path = if file("path").exists() {
        fs::read_to_string(file("path"))
            .map_err(|e| e.to_string())?
            .trim_end_matches(['\r', '\n'])
            .to_owned()
    } else {
        Path::new(name)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
    };
    if file("entities").exists() && !zdiff3 && !detailed {
        let base = fs::read_to_string(&inputs[0]).map_err(|e| e.to_string())?;
        let registry = sem_core::parser::plugins::create_default_registry();
        let plugin = registry
            .get_explicit_plugin(&path)
            .ok_or_else(|| format!("{name}: .entities 需要明确的上游 parser"))?;
        let entities: Vec<_> = plugin
            .extract_entities(&base, &path)
            .into_iter()
            .map(|e| (e.entity_type, e.name))
            .collect();
        let actual = serde_json::to_string_pretty(&entities).map_err(|e| e.to_string())? + "\n";
        compare(
            &file("entities"),
            actual.as_bytes(),
            &artifacts.join(format!("{name}.actual-entities")),
        )?;
    }
    let case = TextCase {
        inputs: inputs.map(Some),
        path,
        default_output: file("output"),
        output: file(output_suffix),
        stderr: file(stderr_suffix),
        exit: file("exit"),
    };
    check_text(
        &case,
        &options,
        &artifacts.join(artifact_name),
        zdiff3,
        detailed,
        None,
    )
}

struct TextCase {
    inputs: [Option<PathBuf>; 3],
    path: String,
    default_output: PathBuf,
    output: PathBuf,
    stderr: PathBuf,
    exit: PathBuf,
}

// Both fixture types exercise the production analysis and diagnostic functions.
fn check_text(
    case: &TextCase,
    options: &common::Options,
    artifact: &Path,
    zdiff3: bool,
    detailed: bool,
    analysis: Option<&Path>,
) -> Result<(), String> {
    let artifact_file = |suffix| PathBuf::from(format!("{}.{suffix}", artifact.display()));
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    for (side, input) in ["base", "ours", "theirs"].iter().zip(&case.inputs) {
        let bytes = match input {
            Some(input) => fs::read(input).map_err(|e| format!("{}: {e}", input.display()))?,
            None => Vec::new(),
        };
        fs::write(scratch.path().join(side), bytes).map_err(|e| e.to_string())?;
    }
    let expected = fs::read(&case.default_output)
        .map_err(|e| format!("{}: {e}", case.default_output.display()))?;
    let expected_code = if case.exit.exists() {
        fs::read_to_string(&case.exit)
            .map_err(|e| e.to_string())?
            .trim()
            .parse::<i32>()
            .map_err(|e| e.to_string())?
    } else {
        // Four files suffice: a marked output means conflict; otherwise clean.
        i32::from(
            expected
                .split(|b| *b == b'\n')
                .any(|line| line.starts_with(b"<<<<<<<")),
        )
    };
    let labels = options.labels();
    let attempt = (|| -> anyhow::Result<_> {
        let inputs = ["base", "ours", "theirs"].map(|name| fs::read(scratch.path().join(name)));
        let inputs = [
            inputs[0].as_ref().unwrap(),
            inputs[1].as_ref().unwrap(),
            inputs[2].as_ref().unwrap(),
        ];
        let texts = [
            strict_weave::merge::validate_text(inputs[0])?,
            strict_weave::merge::validate_text(inputs[1])?,
            strict_weave::merge::validate_text(inputs[2])?,
        ];
        let mut outcome = strict_weave::merge::merge_with_style(
            texts[0],
            texts[1],
            texts[2],
            &case.path,
            &labels,
            options.marker_size,
            if zdiff3 {
                strict_weave::merge::ConflictStyle::Zdiff3
            } else {
                strict_weave::merge::ConflictStyle::Diff3
            },
        )?;
        if let Some(report) = analysis {
            let global = strict_weave::analysis::load_file_analysis(report, &case.path, texts)?;
            strict_weave::analysis::apply_file_analysis(
                &mut outcome,
                &global,
                texts,
                &labels,
                options.marker_size,
            );
        }
        let code = i32::from(outcome.conflicted());
        let diagnostics =
            strict_weave::repository::diagnostics(&case.path, &outcome, &labels, detailed);
        Ok((outcome.content.into_bytes(), code, diagnostics.into_bytes()))
    })();
    let (actual, actual_code, diagnostics) = match attempt {
        Ok(value) => value,
        Err(error) => (
            fs::read(scratch.path().join("ours")).unwrap(),
            129,
            format!("strict-weave：{error:#}\n").into_bytes(),
        ),
    };
    let mut errors = Vec::new();
    if let Err(e) = compare(&case.output, &actual, &artifact_file("actual")) {
        errors.push(e);
    }
    if actual_code != expected_code {
        errors.push(format!(
            "{}: expected exit {expected_code}, actual {:?}\nstderr:\n{}",
            case.path,
            actual_code,
            String::from_utf8_lossy(&diagnostics)
        ));
    }
    if case.stderr.exists() {
        if let Err(e) = compare(&case.stderr, &diagnostics, &artifact_file("actual-stderr")) {
            errors.push(e);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

#[test]
fn text_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let directory = root.join("tests/fixtures");
    let artifacts = root.join("target/fixture-failures");
    let mut names = BTreeSet::new();
    let mut files = Vec::new();
    let mut paths = Vec::new();
    collect_files(&directory, &mut paths);
    paths.sort();
    for path in paths {
        let filename = path
            .strip_prefix(&directory)
            .unwrap()
            .to_str()
            .expect("fixture 文件名必须是 UTF-8")
            .replace('\\', "/");
        let (name, suffix) = filename.rsplit_once('.').expect("fixture 文件缺少后缀");
        assert!(
            matches!(
                suffix,
                "base"
                    | "ours"
                    | "theirs"
                    | "output"
                    | "output-zdiff3"
                    | "path"
                    | "exit"
                    | "stderr"
                    | "stderr-zdiff3"
                    | "stderr-details"
                    | "options"
                    | "analysis"
                    | "entities"
            ),
            "未知 fixture 后缀：{filename}"
        );
        if suffix == "base" {
            names.insert(name.to_owned());
        }
        if suffix == "stderr-zdiff3" {
            assert!(
                directory.join(format!("{name}.output-zdiff3")).exists(),
                "{filename} 缺少对应的 .output-zdiff3"
            );
        }
        files.push(name.to_owned());
    }
    assert!(!names.is_empty(), "没有发现 fixture");
    for name in files {
        assert!(
            names.contains(&name),
            "孤立的 fixture 文件：{name} 缺少 .base"
        );
    }
    let filter = std::env::var("FIXTURE").ok().map(|filter| {
        if names.contains(&filter) {
            return filter;
        }
        let matches: Vec<_> = names
            .iter()
            .filter(|name| Path::new(name).file_name().unwrap().to_str() == Some(&filter))
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "用例 {filter} 不存在或不唯一，请使用相对路径：{matches:?}"
        );
        matches[0].clone()
    });
    if let Some(filter) = &filter {
        assert!(names.contains(filter), "未找到用例 {filter}");
    }
    let mut errors = Vec::new();
    for name in names
        .iter()
        .filter(|name| filter.as_ref().is_none_or(|f| *name == f))
    {
        for zdiff3 in [false, true] {
            if zdiff3 && !directory.join(format!("{name}.output-zdiff3")).exists() {
                continue;
            }
            for detailed in [false, true] {
                if detailed && !directory.join(format!("{name}.stderr-details")).exists() {
                    continue;
                }
                let mode = format!(
                    "{}{}",
                    if zdiff3 { "zdiff3" } else { "default" },
                    if detailed { "/details" } else { "" }
                );
                let result = if directory.join(format!("{name}.output")).is_dir() {
                    multi::check_case(&directory, name, &artifacts, zdiff3, detailed)
                } else {
                    assert!(
                        !directory.join(format!("{name}.analysis")).exists(),
                        "{name}: .analysis 仅支持多文件用例"
                    );
                    check_case(&directory, name, &artifacts, zdiff3, detailed)
                };
                match result {
                    Ok(()) => eprintln!("fixture {name} ({mode}): ok"),
                    Err(error) => errors.push(format!("fixture {name} ({mode}): FAILED\n{error}")),
                }
            }
        }
    }
    assert!(errors.is_empty(), "\n{}", errors.join("\n\n"));
}

fn collect_files(directory: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            let name = entry.file_name();
            let suffix = name
                .to_str()
                .and_then(|s| s.rsplit_once('.'))
                .map(|(_, s)| s);
            if suffix.is_some_and(|s| {
                matches!(
                    s,
                    "base"
                        | "ours"
                        | "theirs"
                        | "output"
                        | "output-zdiff3"
                        | "stderr"
                        | "stderr-zdiff3"
                        | "stderr-details"
                        | "exit"
                )
            }) {
                files.push(entry.path());
            } else {
                collect_files(&entry.path(), files);
            }
        } else {
            assert!(
                kind.is_file(),
                "fixture 必须为普通文件：{}",
                entry.path().display()
            );
            files.push(entry.path());
        }
    }
}

#[test]
fn directory_fixture_discovery_does_not_treat_source_names_as_cases() {
    let temp = tempfile::tempdir().unwrap();
    let base = temp.path().join("group/scenario.base");
    fs::create_dir_all(base.join("src")).unwrap();
    fs::write(base.join("src/nested.base"), b"").unwrap();
    fs::write(temp.path().join("group/single.go.base"), b"").unwrap();
    let mut files = Vec::new();
    collect_files(temp.path(), &mut files);
    files.sort();
    assert_eq!(files, vec![base, temp.path().join("group/single.go.base")]);
}
