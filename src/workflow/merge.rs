use super::*;

pub(super) enum Request {
    Start(Options),
    Resume(Action),
}

pub(super) enum Action {
    Continue,
    Abort,
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FastForward {
    Allow,
    Never,
    Only,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Finish {
    Commit,
    NoCommit,
    Squash,
}

pub(super) struct Options {
    pub target: Option<String>,
    pub ff: FastForward,
    pub finish: Finish,
    pub edit: bool,
    pub message: Option<String>,
    pub detailed: bool,
}

pub(super) fn run(operation: &Operation, request: Request) -> Result<u8> {
    match request {
        Request::Start(options) => start(operation, options),
        Request::Resume(action) => status(git().args([
            "merge",
            match action {
                Action::Continue => "--continue",
                Action::Abort => "--abort",
                Action::Quit => "--quit",
            },
        ])),
    }
}

/// The caller owns the lock, including when invoked after pull's fetch.
pub(super) fn start(operation: &Operation, options: Options) -> Result<u8> {
    idle(&operation.git_dir)?;
    clean()?;
    let ours = commit("HEAD")?;
    let theirs = commit(options.target.as_deref().unwrap_or("@{upstream}"))?;
    let base = merge_base(&ours, &theirs)?;
    let (report, conflicted) = prepare(
        &operation.workspace,
        [&base, &ours, &theirs],
        options.detailed,
    )?;
    if conflicted {
        eprintln!("预分析发现审核项，未执行 git merge；HEAD、index 和工作区保持不变");
        return Ok(1);
    }
    same_head(&ours)?;
    let mut command = operation_git()?;
    // Do not let branch defaults inject a different strategy or favor a side.
    if let Some(branch) = read_optional(&["symbolic-ref", "--quiet", "--short", "HEAD"])? {
        command
            .arg("-c")
            .arg(format!("branch.{branch}.mergeOptions="));
    }
    command.args([
        "-c",
        "merge.autoStash=false",
        "merge",
        "--strategy=ort",
        "--no-autostash",
    ]);
    command.arg(match options.ff {
        FastForward::Only => "--ff-only",
        FastForward::Never => "--no-ff",
        FastForward::Allow => "--ff",
    });
    match options.finish {
        Finish::Squash => {
            command.arg("--squash");
        }
        Finish::Commit => {
            command.args(["--no-squash", "--commit"]);
        }
        Finish::NoCommit => {
            command.args(["--no-squash", "--no-commit"]);
        }
    }
    command.arg(if options.edit { "--edit" } else { "--no-edit" });
    if let Some(message) = options.message {
        command.arg("--message").arg(message);
    }
    command
        .arg("--")
        .arg(theirs)
        .env("STRICT_WEAVE_ANALYSIS", report);
    status(&mut command)
}
