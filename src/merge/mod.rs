//! 单文件 driver 入口：校验输入，完成分析，再只读渲染。
use crate::analysis::local;
use crate::reason::{MoveEvidence, Reason};
use anyhow::{bail, Context, Result};

mod conflict;
mod render;
pub(crate) use conflict::annotate_moves;
pub use conflict::conflict_box;

pub const MAX_BYTES: usize = 1_000_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConflictStyle {
    #[default]
    Diff3,
    Zdiff3,
}

#[derive(Clone, Debug)]
pub struct Labels {
    pub ours: String,
    pub base: String,
    pub theirs: String,
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            ours: "来源未识别".into(),
            base: "来源未识别".into(),
            theirs: "来源未识别".into(),
        }
    }
}

#[derive(Debug)]
pub struct Outcome {
    pub content: String,
    pub reasons: Vec<Reason>,
    /// 非阻断的跨文件关联；有候选不等于有冲突。
    pub related_moves: Vec<MoveEvidence>,
}

impl Outcome {
    pub fn conflicted(&self) -> bool {
        !self.reasons.is_empty()
    }
}

// Prevent multiline labels from becoming source text or terminal control sequences.
pub fn safe_label(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

pub fn validate_text(bytes: &[u8]) -> Result<&str> {
    if bytes.len() > MAX_BYTES {
        bail!("初版仅支持不超过 {MAX_BYTES} bytes 的文本文件");
    }
    if bytes.contains(&0) {
        bail!("初版不支持 binary 文件");
    }
    std::str::from_utf8(bytes).context("初版仅支持 UTF-8 文本")
}

pub(crate) fn validate_inputs(texts: [&str; 3], width: usize) -> Result<()> {
    if !(7..=100).contains(&width) {
        bail!("marker size 必须在 7..=100 之间");
    }
    for text in texts {
        validate_text(text.as_bytes())?;
        // Refuse rather than nest an already conflicted input. This deliberately
        // also rejects source fixtures quoting marker lines.
        if text.lines().any(|line| {
            ["<<<<<<<", "|||||||", "=======", ">>>>>>>"]
                .iter()
                .any(|prefix| line.starts_with(prefix))
        }) {
            bail!("输入已有 conflict marker；保持输入不变，需先处理现有冲突");
        }
    }
    Ok(())
}

pub fn merge(
    base: &str,
    ours: &str,
    theirs: &str,
    path: &str,
    labels: &Labels,
    width: usize,
) -> Result<Outcome> {
    merge_with_style(
        base,
        ours,
        theirs,
        path,
        labels,
        width,
        ConflictStyle::Diff3,
    )
}

/// Complete all conflict analysis before selecting its display.
pub fn merge_with_style(
    base: &str,
    ours: &str,
    theirs: &str,
    path: &str,
    labels: &Labels,
    width: usize,
    style: ConflictStyle,
) -> Result<Outcome> {
    let analysis = local::analyze(base, ours, theirs, path, labels, width, style)?;
    let content = render::render(&analysis, [base, ours, theirs], labels, width);
    Ok(Outcome {
        content,
        reasons: analysis.reasons,
        related_moves: Vec::new(),
    })
}
