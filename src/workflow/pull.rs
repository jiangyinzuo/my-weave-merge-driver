use super::*;
use merge::{FastForward, Finish};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Merge,
    Rebase,
    Interactive,
    RebaseMerges,
}

pub(super) struct Options {
    pub remote: Option<String>,
    pub branch: Option<String>,
    pub mode: Option<Mode>,
    pub ff: FastForward,
    pub finish: Finish,
    pub detailed: bool,
}

fn configured_mode(branch: &str) -> Result<Mode> {
    let value = match read_optional(&["config", "--get", &format!("branch.{branch}.rebase")])? {
        Some(value) => value,
        None => {
            read_optional(&["config", "--get", "pull.rebase"])?.unwrap_or_else(|| "false".into())
        }
    };
    match value.as_str() {
        "true" => Ok(Mode::Rebase),
        "false" => Ok(Mode::Merge),
        "interactive" => Ok(Mode::Interactive),
        "merges" => Ok(Mode::RebaseMerges),
        _ => bail!("不支持的 pull.rebase 配置：{value}"),
    }
}

pub(super) fn run(operation: &Operation, options: Options) -> Result<u8> {
    idle(&operation.git_dir)?;
    clean()?;
    let ours = commit("HEAD")?;
    let local = read(&["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let remote = match options.remote {
        Some(remote) => remote,
        None => read(&["config", "--get", &format!("branch.{local}.remote")])?,
    };
    let branch = match options.branch {
        Some(branch) => branch,
        None => read(&["config", "--get", &format!("branch.{local}.merge")])?,
    };
    if branch.contains([':', '*', '\n']) || branch.starts_with('-') {
        bail!("pull 只支持单个远端 branch/ref");
    }
    let mode = match options.mode {
        Some(mode) => mode,
        None => configured_mode(&local)?,
    };
    if mode != Mode::Merge && (options.ff == FastForward::Never || options.finish != Finish::Commit)
    {
        bail!("rebase 模式不能使用 merge 专用选项");
    }
    let code = status(git().args([
        "fetch",
        "--no-tags",
        "--no-recurse-submodules",
        "--",
        &remote,
        &branch,
    ]))?;
    if code != 0 {
        return Ok(code);
    }
    let theirs = commit("FETCH_HEAD")?;
    same_head(&ours)?;
    if mode == Mode::Merge || options.ff == FastForward::Only {
        merge::start(
            operation,
            merge::Options {
                target: Some(theirs),
                ff: options.ff,
                finish: options.finish,
                edit: false,
                message: None,
                detailed: options.detailed,
            },
        )
    } else {
        rebase::start(
            operation,
            rebase::Options {
                upstream: rebase::Upstream::Revision(Some(theirs)),
                branch: None,
                onto: None,
                interactive: mode == Mode::Interactive,
                rebase_merges: mode == Mode::RebaseMerges,
                detailed: options.detailed,
            },
        )
    }
}
