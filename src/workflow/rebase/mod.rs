//! Native Git orchestration for guarded rebase.
use super::*;
mod callbacks;
mod state;
mod todo;
pub use callbacks::{check, edit_todo};
use state::{load, state_path, State};
use todo::{guarded, skip_blocked};

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

pub(super) enum Request {
    Start(Options),
    Resume(Action),
}
#[derive(Clone, Copy)]
pub(super) enum Action {
    Continue,
    Abort,
    Skip,
    Quit,
    EditTodo,
    ShowCurrentPatch,
}
pub(super) enum Upstream {
    Root,
    Revision(Option<String>),
}
pub(super) struct Options {
    pub upstream: Upstream,
    pub branch: Option<String>,
    pub onto: Option<String>,
    pub interactive: bool,
    pub rebase_merges: bool,
    pub detailed: bool,
}

pub(super) fn run(operation: &Operation, request: Request) -> Result<u8> {
    match request {
        Request::Start(options) => start(operation, options),
        Request::Resume(action) => resume(operation, action),
    }
}

fn resume(operation: &Operation, action: Action) -> Result<u8> {
    let directory = &operation.git_dir;
    let mut state = load()?;
    if matches!(action, Action::EditTodo) {
        state.editor = Some(read(&["var", "GIT_SEQUENCE_EDITOR"])?);
        state.save()?;
    }
    let mut command = command(&state)?;
    match action {
        Action::Abort => {
            command.arg("--abort");
        }
        Action::Quit => {
            command.arg("--quit");
        }
        Action::EditTodo => {
            command.arg("--edit-todo");
        }
        Action::ShowCurrentPatch => {
            command.arg("--show-current-patch");
        }
        Action::Continue | Action::Skip => {
            let todo_path = directory.join("rebase-merge/git-rebase-todo");
            let mut todo = fs::read_to_string(&todo_path)?;
            let blocked = state.blocked_path();
            if matches!(action, Action::Skip) && blocked.exists() {
                clean()?;
                let line = fs::read_to_string(&blocked)?;
                todo = skip_blocked(&state.executable, &todo, &line, commit)?.into();
                command.arg("--continue");
            } else {
                command.arg(if matches!(action, Action::Skip) {
                    "--skip"
                } else {
                    "--continue"
                });
            }
            // Revalidate manual todo edits; never continue an unguarded pick.
            repository::atomic_write(&todo_path, guarded(&state.executable, &todo)?.as_bytes())?;
            if matches!(action, Action::Skip) && blocked.exists() {
                fs::remove_file(blocked)?;
            }
        }
    }
    state.finish(status(&mut command))
}

pub(super) fn start(operation: &Operation, options: Options) -> Result<u8> {
    let directory = &operation.git_dir;
    let workspace = &operation.workspace;
    idle(directory)?;
    clean()?;
    if state_path(directory).exists() {
        bail!("存在未清理的 strict-weave rebase 状态；请检查后使用 --quit 清理");
    }
    let original = commit("HEAD")?;
    let upstream = match &options.upstream {
        Upstream::Root => None,
        Upstream::Revision(revision) => Some(commit(revision.as_deref().unwrap_or("@{upstream}"))?),
    };
    let onto = options.onto.as_deref().map(commit).transpose()?;
    if let Some(branch) = &options.branch {
        read(&["show-ref", "--verify", &format!("refs/heads/{branch}")])?;
    }
    let editor = if options.interactive {
        Some(read(&["var", "GIT_SEQUENCE_EDITOR"])?)
    } else {
        None
    };
    let folder = tempfile::Builder::new()
        .prefix("rebase-")
        .tempdir_in(workspace)?;
    let state = State {
        git_dir: directory.clone(),
        directory: folder.keep(),
        executable: executable()?,
        editor,
        detailed: options.detailed,
    };
    same_head(&original)?;
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
    command.arg(if options.rebase_merges {
        "--rebase-merges"
    } else {
        "--no-rebase-merges"
    });
    if let Some(onto) = onto {
        command.arg("--onto").arg(onto);
    }
    if matches!(options.upstream, Upstream::Root) {
        command.arg("--root");
    }
    command.arg("--");
    if let Some(upstream) = upstream {
        command.arg(upstream);
    }
    if let Some(branch) = options.branch {
        command.arg(branch);
    }
    state.save()?;
    state.finish(status(&mut command))
}
