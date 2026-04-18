//! Synthesizer traits — the universal rendering interface.
//!
//! Trait surface mirrors `arch-synthesizer::traits` (MIT-licensed sibling
//! crate at `pleme-io/arch-synthesizer`): `Synthesizer` for
//! Input→AST→Output morphisms, `MultiSynthesizer` for multi-file emission,
//! `Artifact` for a path+content pair. We restate them here (rather than
//! importing arch-synthesizer) to avoid pulling its path-dep chain
//! (nix-synthesizer, yaml-synthesizer, helm-synthesizer, …) into every
//! consumer of tatara-nix.
//!
//! Any tatara-lisp-authored domain can implement `Synthesizer` to plug into
//! the arch-synthesizer rendering pipeline without friction — the trait
//! shapes line up exactly.

/// Universal rendering morphism: proven input → AST → emitted output.
/// Deterministic, total, composable.
pub trait Synthesizer {
    type Input;
    type Ast;
    type Output;

    fn synthesize(&self, input: &Self::Input) -> Self::Ast;
    fn render(&self, ast: &Self::Ast) -> Self::Output;

    fn generate(&self, input: &Self::Input) -> Self::Output {
        let ast = self.synthesize(input);
        self.render(&ast)
    }
}

/// Multi-file synthesizer — emits a set of (path, content) pairs.
/// `Input: ?Sized` lets concrete callers pass trait objects (e.g., a
/// `&dyn PackageSet`) as input.
pub trait MultiSynthesizer {
    type Input: ?Sized;
    fn generate_all(&self, input: &Self::Input) -> Vec<Artifact>;
}

/// One emitted file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub path: String,
    pub content: String,
}

impl Artifact {
    pub fn new(path: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Upper;
    impl Synthesizer for Upper {
        type Input = String;
        type Ast = String;
        type Output = String;
        fn synthesize(&self, s: &String) -> String {
            s.to_uppercase()
        }
        fn render(&self, ast: &String) -> String {
            format!("// rendered\n{ast}")
        }
    }

    #[test]
    fn generate_composes_synthesize_then_render() {
        let u = Upper;
        assert_eq!(u.generate(&"hello".into()), "// rendered\nHELLO");
    }

    struct TwoFile;
    impl MultiSynthesizer for TwoFile {
        type Input = &'static str;
        fn generate_all(&self, input: &&'static str) -> Vec<Artifact> {
            vec![
                Artifact::new("a.txt", input.to_string()),
                Artifact::new("b.txt", input.chars().rev().collect::<String>()),
            ]
        }
    }

    #[test]
    fn multi_synthesizer_emits_several_artifacts() {
        let out = TwoFile.generate_all(&"hi");
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].path, "a.txt");
        assert_eq!(out[0].content, "hi");
        assert_eq!(out[1].content, "ih");
    }
}
