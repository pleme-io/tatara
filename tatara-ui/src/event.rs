//! `UiEvent` — every runtime event that the renderer may paint.
//!
//! Events serialize to tatara-lisp S-expressions, so a stream of events is
//! itself content-addressable: BLAKE3 over the canonical JSON of the stream
//! gives you a **run-identity hash** that `tatara replay <hash>` can use to
//! reproduce the exact Nord output of a past invocation.

use serde::{Deserialize, Serialize};

use crate::palette::Role;

/// 7-character BLAKE3 prefix, rendered in dim next to every artifact line.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShortHash(pub String);

impl ShortHash {
    pub fn from_blake3_hex(full: &str) -> Self {
        Self(full.chars().take(7).collect())
    }
}

impl std::fmt::Display for ShortHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Every paintable thing the toolchain does. Variants are deliberately
/// small — the renderer owns the prose so themes control every word.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum UiEvent {
    /// The hero banner. `❄ tatara — <title>` …
    Banner {
        title: String,
        subtitle: Option<String>,
    },
    /// Section divider. `⟡ <title>` with an underline of dim glyphs.
    Section {
        title: String,
    },
    /// Tagged log line. `level` picks the sigil + color.
    Log {
        level: LogLevel,
        message: String,
    },
    /// Phase start — begins a timed scope. `(realize/begin …)` in the stream.
    PhaseBegin {
        phase: String,
    },
    /// Phase finish. `elapsed_ms` is what we paint next to the sigil.
    PhaseEnd {
        phase: String,
        elapsed_ms: u64,
    },
    /// An artifact line — `❄ name  [blake3:xxxxxxx]  <state>`.
    Artifact {
        name: String,
        hash: ShortHash,
        state: ArtifactState,
    },
    /// Summary / content-root banner at the end of a run.
    Summary {
        root_hash: ShortHash,
        total: usize,
        built: usize,
        cached: usize,
        failed: usize,
    },
    /// Free-form key/value table row — for `tatara cache show` and friends.
    Row {
        cells: Vec<Cell>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LogLevel {
    Info,
    Success,
    Warn,
    Error,
    Dim,
}

impl LogLevel {
    pub fn role(self) -> Role {
        match self {
            Self::Info => Role::Info,
            Self::Success => Role::Success,
            Self::Warn => Role::Warn,
            Self::Error => Role::Error,
            Self::Dim => Role::Dim,
        }
    }
}

/// Cache-aware artifact state — the "fun" in "cachable declarative systems".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum ArtifactState {
    /// Freshly built — prints elapsed time in a Clock sigil.
    Built { elapsed_ms: u64 },
    /// Cache hit — prints a Lightning sigil and no elapsed time.
    Cached,
    /// Queued but not yet started — prints a hollow dot.
    Pending,
    /// Build failure — prints a Cross sigil.
    Failed { reason: String },
}

impl ArtifactState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Built { .. } => "built",
            Self::Cached => "cached",
            Self::Pending => "pending",
            Self::Failed { .. } => "failed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cell {
    pub text: String,
    #[serde(default)]
    pub role: Option<Role>,
}

impl Cell {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            role: None,
        }
    }

    pub fn with_role(text: impl Into<String>, role: Role) -> Self {
        Self {
            text: text.into(),
            role: Some(role),
        }
    }
}

/// The ordered log a runner accumulates while working. Serializable for
/// `tatara replay <hash>` — the whole stream is content-addressable.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EventStream {
    pub events: Vec<UiEvent>,
}

impl EventStream {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, e: UiEvent) {
        self.events.push(e);
    }

    /// BLAKE3 of the canonical JSON — the run-identity hash.
    pub fn run_hash(&self) -> String {
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        hex::encode(blake3::hash(&bytes).as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_hash_is_seven_chars() {
        let sh = ShortHash::from_blake3_hex(
            "cxx3i50lvlprhlqclm1mxmnp77bawjbx-fake-ignored",
        );
        assert_eq!(sh.0.len(), 7);
        assert_eq!(sh.to_string(), "cxx3i50");
    }

    #[test]
    fn artifact_state_labels() {
        assert_eq!(ArtifactState::Cached.label(), "cached");
        assert_eq!(
            ArtifactState::Built { elapsed_ms: 100 }.label(),
            "built"
        );
    }

    #[test]
    fn stream_run_hash_is_deterministic() {
        let mut s = EventStream::new();
        s.push(UiEvent::Section {
            title: "boot".into(),
        });
        s.push(UiEvent::Log {
            level: LogLevel::Info,
            message: "hello".into(),
        });
        let h1 = s.run_hash();
        let h2 = s.run_hash();
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }
}
