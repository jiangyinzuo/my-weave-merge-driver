//! Flat, text-only test cases: NAME.base / .ours / .theirs / .output.
//! Optional NAME.path, .exit and .stderr specify path, exit status and report.
//! NAME.output-zdiff3 adds a second run with --zdiff3 and the same exit status.
//! NAME.stderr-details verifies --explain-reasons in every available style;
//! output bytes and exit status must stay identical to that style's baseline.
//! NAME.options optionally overrides marker_size and the three labels via JSON.
use std::{collections::BTreeSet, fs, path::Path, process::Command};
mod common;

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
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    for side in ["base", "ours", "theirs"] {
        fs::copy(file(side), scratch.path().join(side))
            .map_err(|e| format!("{name}.{side}: {e}"))?;
    }
    let path = if file("path").exists() {
        fs::read_to_string(file("path"))
            .map_err(|e| e.to_string())?
            .trim_end_matches(['\r', '\n'])
            .to_owned()
    } else {
        name.to_owned()
    };
    let expected = fs::read(file("output")).map_err(|e| format!("{name}.output: {e}"))?;
    let expected_code = if file("exit").exists() {
        fs::read_to_string(file("exit"))
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
    let mut command = Command::new(env!("CARGO_BIN_EXE_strict-weave"));
    if zdiff3 {
        command.arg("driver").arg("--zdiff3");
    } else {
        command.arg("driver");
    }
    if detailed {
        command.arg("--explain-reasons");
    }
    let result = command
        .current_dir(scratch.path())
        .args([
            "base",
            "ours",
            "theirs",
            &path,
            &options.marker_size.to_string(),
        ])
        .arg(format!("--ours-label={}", options.ours_label))
        .arg(format!("--base-label={}", options.base_label))
        .arg(format!("--theirs-label={}", options.theirs_label))
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("GIT_CONFIG_COUNT")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("STRICT_WEAVE_ANALYSIS")
        .output()
        .map_err(|e| e.to_string())?;
    let actual = fs::read(scratch.path().join("ours")).map_err(|e| e.to_string())?;
    let mut errors = Vec::new();
    if let Err(e) = compare(
        &file(output_suffix),
        &actual,
        &artifacts.join(format!("{artifact_name}.actual")),
    ) {
        errors.push(e);
    }
    if result.status.code() != Some(expected_code) {
        errors.push(format!(
            "{name}: expected exit {expected_code}, actual {:?}\nstderr:\n{}",
            result.status,
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    if !result.stdout.is_empty() {
        errors.push(format!("{name}: driver 不应向 stdout 输出内容"));
    }
    if file(stderr_suffix).exists() {
        if let Err(e) = compare(
            &file(stderr_suffix),
            &result.stderr,
            &artifacts.join(format!("{artifact_name}.actual-stderr")),
        ) {
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
    for entry in fs::read_dir(&directory).unwrap() {
        let entry = entry.unwrap();
        assert!(
            entry.file_type().unwrap().is_file(),
            "fixtures 必须平铺：{}",
            entry.path().display()
        );
        let filename = entry
            .file_name()
            .into_string()
            .expect("fixture 文件名必须是 UTF-8");
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
            ),
            "未知 fixture 后缀：{filename}"
        );
        if suffix == "base" {
            names.insert(name.to_owned());
        }
        if suffix == "stderr-zdiff3" {
            assert!(
                directory.join(format!("{name}.output-zdiff3")).is_file(),
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
    let filter = std::env::var("FIXTURE").ok();
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
                match check_case(&directory, name, &artifacts, zdiff3, detailed) {
                    Ok(()) => eprintln!("fixture {name} ({mode}): ok"),
                    Err(error) => errors.push(format!("fixture {name} ({mode}): FAILED\n{error}")),
                }
            }
        }
    }
    assert!(errors.is_empty(), "\n{}", errors.join("\n\n"));
}
