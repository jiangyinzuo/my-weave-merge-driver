//! Sequencer callbacks execute under the parent operation lock.
use super::super::*;
use super::state::load;
use super::todo::{editable, guarded, instruction, Instruction};

pub fn edit_todo(path: &Path) -> Result<u8> {
    let state = load()?;
    let text = fs::read_to_string(path)?;
    repository::atomic_write(path, editable(&state.executable, &text).as_bytes())?;
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
    let result = guarded(&state.executable, &fs::read_to_string(path)?)?;
    repository::atomic_write(path, result.as_bytes())?;
    Ok(0)
}

pub fn check(line: &str) -> Result<u8> {
    let state = load()?;
    if !state.git_dir.join("rebase-merge").is_dir() {
        bail!("没有进行中的 merge-backend rebase");
    }
    // Every failure leaves enough information for guarded --skip; --continue
    // retries this exec through Git's reschedule-failed-exec mechanism.
    repository::atomic_write(&state.blocked_path(), line.as_bytes())?;
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
            (merge_base(&ours, &theirs)?, theirs)
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
    state.activate(&report)?;
    Ok(0)
}
