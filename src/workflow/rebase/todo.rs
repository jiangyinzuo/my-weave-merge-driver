//! Todo parsing and transformations; revision resolution is supplied by the caller.
use super::super::shell_quote;
use anyhow::{bail, Context, Result};

pub(super) fn guard(executable: &str, instruction: &str) -> String {
    format!(
        "exec {} rebase-check {}",
        shell_quote(executable),
        shell_quote(instruction)
    )
}

/// Git's sequencer remains responsible for all history edits. Guards are
/// inserted before replay commands, never substituted for those commands.
pub(super) fn guarded(executable: &str, todo: &str) -> Result<String> {
    let prefix = format!("exec {} rebase-check ", shell_quote(executable));
    let mut result = String::new();
    for line in todo.lines() {
        let line = line.trim();
        if line.starts_with(&prefix) {
            continue;
        }
        let command = line.split_whitespace().next().unwrap_or("");
        if matches!(
            command,
            "pick"
                | "p"
                | "reword"
                | "r"
                | "edit"
                | "e"
                | "squash"
                | "s"
                | "fixup"
                | "f"
                | "merge"
                | "m"
        ) {
            // Validate supported instruction shapes before Git starts replay.
            instruction(line)?;
            result.push_str(&guard(executable, line));
            result.push('\n');
        } else if !line.is_empty()
            && !line.starts_with('#')
            && !matches!(
                command,
                "label"
                    | "l"
                    | "reset"
                    | "t"
                    | "break"
                    | "b"
                    | "exec"
                    | "x"
                    | "drop"
                    | "d"
                    | "update-ref"
                    | "u"
                    | "noop"
            )
        {
            bail!("不支持的 rebase todo 指令：{command}");
        }
        result.push_str(line);
        result.push('\n');
    }
    Ok(result)
}

pub(super) enum Instruction<'a> {
    Pick(&'a str),
    Merge(&'a str),
}
pub(super) fn instruction(line: &str) -> Result<Instruction<'_>> {
    let mut fields = line.split_whitespace();
    let command = fields.next().context("空 rebase 指令")?;
    let mut value = fields.next().context("rebase 指令缺少参数")?;
    if matches!(value, "-C" | "-c") {
        let original = fields.next().context("缺少原 commit")?;
        value = if matches!(command, "merge" | "m") {
            fields.next().context("缺少 merge 目标")?
        } else {
            original
        };
    }
    if matches!(command, "merge" | "m") {
        if value.starts_with('-')
            || value.starts_with('#')
            || fields.next().is_some_and(|v| !v.starts_with('#'))
        {
            bail!("rebase merge 只支持单个目标；不支持 octopus 或自定义 merge 参数");
        }
        Ok(Instruction::Merge(value))
    } else {
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("rebase pick 必须使用 commit hash");
        }
        Ok(Instruction::Pick(value))
    }
}

pub(super) fn editable(executable: &str, text: &str) -> String {
    let prefix = format!("exec {} rebase-check ", shell_quote(executable));
    text.lines()
        .filter(|line| !line.trim().starts_with(&prefix))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// The guard must match exactly. Git may expand the following instruction's
/// short commit hash after stopping; resolve hashes before comparing that step.
/// All other command fields must match, including flags, labels and message.
pub(super) fn skip_blocked<'a>(
    executable: &str,
    todo: &'a str,
    line: &str,
    resolve_commit: impl Fn(&str) -> Result<String>,
) -> Result<&'a str> {
    const CHANGED: &str = "待办列表已变化，无法确认要跳过的步骤；请使用 --edit-todo 或 --abort";
    let prefix = format!("{}\n", guard(executable, line));
    let remaining = todo.trim_start().strip_prefix(&prefix).context(CHANGED)?;
    let (actual, remaining) = remaining.split_once('\n').context(CHANGED)?;
    if actual != line {
        let canonical = |text: &str| -> Result<Vec<String>> {
            instruction(text)?;
            let mut fields: Vec<String> = text.split_whitespace().map(String::from).collect();
            // A merge without -C/-c has a label, not an original commit hash.
            let index = if matches!(fields[1].as_str(), "-C" | "-c") {
                Some(2)
            } else if matches!(fields[0].as_str(), "merge" | "m") {
                None
            } else {
                Some(1)
            };
            if let Some(index) = index {
                fields[index] = resolve_commit(&fields[index])?;
            }
            Ok(fields)
        };
        if canonical(line)? != canonical(actual)? {
            bail!(CHANGED);
        }
    }
    Ok(remaining)
}
