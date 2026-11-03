//! Shared result schema returned by every checker. One shape → one frontend
//! component. Kept minimal and stable; individual checkers only produce
//! [`Finding`]s and a summary — the score/grade is derived in [`crate::score`].

use serde::{Deserialize, Serialize};

/// Severity of a single finding. `weight()` drives scoring in [`crate::score`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    Warning,
    Info,
}

impl Severity {
    /// Points deducted from a perfect score for one finding of this severity.
    /// A single Critical alone fails the audit (see tests in [`crate::score`]).
    pub fn weight(self) -> u32 {
        match self {
            Severity::Critical => 50,
            Severity::Warning => 15,
            Severity::Info => 3,
        }
    }
}

/// Letter grade derived from the 0..=100 score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Grade {
    A,
    B,
    C,
    D,
    F,
}

impl Grade {
    pub fn from_score(score: u32) -> Grade {
        match score {
            90..=u32::MAX => Grade::A,
            80..=89 => Grade::B,
            70..=79 => Grade::C,
            60..=69 => Grade::D,
            _ => Grade::F,
        }
    }
}

/// A single issue surfaced by a checker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    /// Stable machine id, e.g. "compose.image.unpinned_tag".
    pub id: String,
    pub severity: Severity,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    /// What the finding is about, e.g. a compose service name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs_url: Option<String>,
}

impl Finding {
    pub fn new(id: impl Into<String>, severity: Severity, title: impl Into<String>) -> Self {
        Finding {
            id: id.into(),
            severity,
            title: title.into(),
            detail: None,
            remediation: None,
            target: None,
            docs_url: None,
        }
    }

    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn with_remediation(mut self, remediation: impl Into<String>) -> Self {
        self.remediation = Some(remediation.into());
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Call-to-action shown on the result page (e.g. "fix & deploy on TryDirect").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cta {
    pub label: String,
    pub url: String,
}

/// The uniform response every checker returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditReport {
    /// Which checker produced this, e.g. "compose".
    pub checker: String,
    pub score: u32,
    pub grade: Grade,
    pub summary: String,
    pub findings: Vec<Finding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cta: Option<Cta>,
}
