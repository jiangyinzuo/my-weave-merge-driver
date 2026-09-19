use super::*;
use clap::{ArgGroup, Args};
use serde::{Deserialize, Serialize};

#[derive(Args)]
#[command(group(ArgGroup::new("action").args(["continue_rebase", "abort", "skip", "quit", "edit_todo", "show_current_patch"])))]
pub struct RebaseArgs {
    pub upstream: Option<String>,
    /// 可选的本地分支；省略时使用当前 HEAD
    pub branch: Option<String>,
    #[arg(long)]
    pub onto: Option<String>,
    #[arg(long, conflicts_with = "upstream")]
    pub root: bool,
    #[arg(short, long)]
    pub interactive: bool,
    #[arg(short = 'r', long)]
    pub rebase_merges: bool,
    #[arg(long)]
    pub explain_reasons: bool,
    #[arg(long = "continue")]
    pub continue_rebase: bool,
    #[arg(long)]
    pub abort: bool,
    #[arg(long)]
    pub skip: bool,
    #[arg(long)]
    pub quit: bool,
    #[arg(long)]
    pub edit_todo: bool,
    #[arg(long)]
    pub show_current_patch: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    git_dir: PathBuf,
    directory: PathBuf,
    executable: String,
    editor: Option<String>,
    detailed: bool,
}

fn state_path(directory: &Path) -> PathBuf {
    directory.join("strict-weave/rebase.json")
}

fn load() -> Result<State> {
    let directory = git_dir()?;
    let state: State = serde_json::from_slice(
        &fs::read(state_path(&directory))
            .context("没有 strict-weave rebase 状态；请使用原生 Git 处理非本工具启动的 rebase")?,
    )?;
    if state.git_dir != directory || !state.directory.starts_with(directory.join("strict-weave")) {
        bail!("rebase 状态与当前仓库不匹配");
    }
    Ok(state)
}

fn guard(state: &State, instruction: &str) -> String {
    format!(
        "exec {} rebase-check {}",
        shell_quote(&state.executable),
        shell_quote(instruction)
    )
}

/// Git's sequencer remains responsible for all history edits. Guards are
/// inserted before replay commands, never substituted for those commands.
fn guarded(state: &State, todo: &str) -> Result<String> {
    let prefix = format!("exec {} rebase-check ", shell_quote(&state.executable));
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
            result.push_str(&guard(state, line));
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

enum Instruction<'a> {
    Pick(&'a str),
    Merge(&'a str),
}
fn instruction(line: &str) -> Result<Instruction<'_>> {
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

pub fn edit_todo(path: &Path) -> Result<u8> {
    let state = load()?;
    let text = fs::read_to_string(path)?;
    let prefix = format!("exec {} rebase-check ", shell_quote(&state.executable));
    let editable = text
        .lines()
        .filter(|l| !l.trim().starts_with(&prefix))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(path, editable)?;
    if let Some(editor) = &state.editor {
        let code = status(
            Command::new("sh")
                .args(["-c", &format!("{editor} \"$1\""), "strict-weave-editor"])
                .arg(path),
        )?;
        if code != 0 {
            return Ok(code);
        }
    }
    let result = guarded(&state, &fs::read_to_string(path)?)?;
    fs::write(path, result)?;
    Ok(0)
}

pub fn check(line: &str) -> Result<u8> {
    let state = load()?;
    if !state.git_dir.join("rebase-merge").is_dir() {
        bail!("没有进行中的 merge-backend rebase");
    }
    // Every failure leaves enough information for guarded --skip; --continue
    // retries this exec through Git's reschedule-failed-exec mechanism.
    repository::atomic_write(&state.directory.join("blocked"), line.as_bytes())?;
    clean()?;
    let ours = commit("HEAD")?;
    let (base, theirs) = match instruction(line)? {
        Instruction::Pick(value) => {
            let theirs = commit(value)?;
            let parents = read(&["rev-list", "--parents", "-n", "1", &theirs])?;
            let parents: Vec<_> = parents.split_whitespace().skip(1).collect();
            let base = match parents.as_slice() {
                [] => empty_tree()?,
                [parent] => (*parent).into(),
                _ => bail!("不能把 merge commit 当作普通 pick；请使用 --rebase-merges"),
            };
            (base, theirs)
        }
        Instruction::Merge(label) => {
            let theirs = commit(&format!("refs/rewritten/{label}"))?;
            let bases = read(&["merge-base", "--all", &ours, &theirs])?;
            let bases: Vec<_> = bases.lines().collect();
            if bases.len() != 1 {
                bail!("rebase merge 需要唯一 merge base");
            }
            (bases[0].into(), theirs)
        }
    };
    let (report, conflicted) = prepare(&state.directory, [&base, &ours, &theirs], state.detailed)?;
    if conflicted {
        eprintln!("预分析发现审核项，尚未应用该步骤。可查看报告，使用 strict-weave rebase --skip 跳过，或 --abort 中止；--continue 会重新检查。");
        return Ok(1);
    }
    same_head(&ours)?;
    // The driver reads this atomically replaced snapshot during this step.
    // Archived reports stay immutable; no reader observes a partial report.
    repository::atomic_write(&state.directory.join("active.json"), &fs::read(report)?)?;
    let active = state.directory.join("active.json");
    let mut permissions = fs::metadata(&active)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(active, permissions)?;
    fs::remove_file(state.directory.join("blocked"))?;
    Ok(0)
}

fn command(state: &State) -> Result<Command> {
    let mut command = operation_git()?;
    command.args([
        "-c",
        "rebase.abbreviateCommands=false",
        "-c",
        "rebase.instructionFormat=%s",
        "-c",
        "rebase.autoSquash=false",
        "-c",
        "rebase.autoStash=false",
        "-c",
        "rebase.updateRefs=false",
    ]);
    command.env(
        "GIT_SEQUENCE_EDITOR",
        format!("{} rebase-todo", shell_quote(&state.executable)),
    );
    command.env("STRICT_WEAVE_ANALYSIS", state.directory.join("active.json"));
    command.arg("rebase");
    Ok(command)
}

fn finish(state: &State, code: u8) -> Result<u8> {
    if !state.git_dir.join("rebase-merge").exists() && !state.git_dir.join("rebase-apply").exists()
    {
        fs::remove_file(state_path(&state.git_dir))?;
    }
    Ok(code)
}

pub fn run(args: RebaseArgs) -> Result<u8> {
    let directory = git_dir()?;
    let workspace = workspace(&directory)?;
    let _lock = Lock::acquire(&workspace)?;
    let action = args.continue_rebase
        || args.abort
        || args.skip
        || args.quit
        || args.edit_todo
        || args.show_current_patch;
    if action {
        if args.upstream.is_some()
            || args.branch.is_some()
            || args.onto.is_some()
            || args.root
            || args.interactive
            || args.rebase_merges
        {
            bail!("rebase 继续/中止不能同时指定新操作参数");
        }
        let mut state = load()?;
        if args.edit_todo {
            // The current invocation chooses the editor, as native Git does.
            // Ordinary rebases initially suppress editing, but --edit-todo
            // must still open the user's chosen editor.
            state.editor = Some(read(&["var", "GIT_SEQUENCE_EDITOR"])?);
            repository::atomic_write(&state_path(&directory), &serde_json::to_vec_pretty(&state)?)?;
        }
        let mut command = command(&state)?;
        if args.abort {
            command.arg("--abort");
        } else if args.quit {
            command.arg("--quit");
        } else if args.edit_todo {
            command.arg("--edit-todo");
        } else if args.show_current_patch {
            command.arg("--show-current-patch");
        } else {
            let todo_path = directory.join("rebase-merge/git-rebase-todo");
            let mut todo = fs::read_to_string(&todo_path)?;
            let blocked = state.directory.join("blocked");
            if args.skip && blocked.exists() {
                clean()?;
                let line = fs::read_to_string(&blocked)?;
                let expected = format!("{}\n{}\n", guard(&state, &line), line);
                let trimmed = todo.trim_start();
                if !trimmed.starts_with(&expected) {
                    bail!("待办列表已变化，无法确认要跳过的步骤；请使用 --edit-todo 或 --abort");
                }
                todo = trimmed[expected.len()..].into();
                fs::remove_file(blocked)?;
                command.arg("--continue");
            } else {
                command.arg(if args.skip { "--skip" } else { "--continue" });
            }
            // Revalidate manual todo edits; never continue an unguarded pick.
            fs::write(todo_path, guarded(&state, &todo)?)?;
        }
        let code = status(&mut command)?;
        return finish(&state, code);
    }
    idle(&directory)?;
    clean()?;
    if state_path(&directory).exists() {
        bail!("存在未清理的 strict-weave rebase 状态；请检查后使用 --quit 清理");
    }
    let original = commit("HEAD")?;
    let upstream = if args.root {
        None
    } else {
        Some(commit(args.upstream.as_deref().unwrap_or("@{upstream}"))?)
    };
    let onto = args.onto.as_deref().map(commit).transpose()?;
    if let Some(branch) = &args.branch {
        read(&["show-ref", "--verify", &format!("refs/heads/{branch}")])?;
    }
    let editor = if args.interactive {
        Some(read(&["var", "GIT_SEQUENCE_EDITOR"])?)
    } else {
        None
    };
    let folder = tempfile::Builder::new()
        .prefix("rebase-")
        .tempdir_in(&workspace)?;
    let state = State {
        git_dir: directory.clone(),
        directory: folder.keep(),
        executable: executable()?,
        editor,
        detailed: args.explain_reasons,
    };
    same_head(&original)?;
    repository::atomic_write(&state_path(&directory), &serde_json::to_vec_pretty(&state)?)?;
    let mut command = command(&state)?;
    command.args([
        "--interactive",
        "--merge",
        "--strategy=ort",
        "--no-autostash",
        "--no-autosquash",
        "--no-update-refs",
        "--reschedule-failed-exec",
        "--no-fork-point",
        "--reapply-cherry-picks",
        "--empty=keep",
        "--keep-empty",
        "--force-rebase",
    ]);
    command.arg(if args.rebase_merges {
        "--rebase-merges"
    } else {
        "--no-rebase-merges"
    });
    if let Some(onto) = onto {
        command.arg("--onto").arg(onto);
    }
    if args.root {
        command.arg("--root");
    }
    command.arg("--");
    if let Some(upstream) = upstream {
        command.arg(upstream);
    }
    if let Some(branch) = args.branch {
        command.arg(branch);
    }
    let code = status(&mut command)?;
    finish(&state, code)
}
