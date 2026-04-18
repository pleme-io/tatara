//! Lexically-scoped environment — chained maps.

use std::collections::HashMap;

use crate::ast::Sexp;

/// A lexical environment. `Env::extend` produces a child scope; symbol
/// lookup walks the chain from innermost outward.
#[derive(Debug, Default, Clone)]
pub struct Env {
    frames: Vec<HashMap<String, Sexp>>,
}

impl Env {
    pub fn new() -> Self {
        Self {
            frames: vec![HashMap::new()],
        }
    }

    pub fn push(&mut self) {
        self.frames.push(HashMap::new());
    }

    pub fn pop(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    pub fn define(&mut self, name: impl Into<String>, value: Sexp) {
        if let Some(top) = self.frames.last_mut() {
            top.insert(name.into(), value);
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&Sexp> {
        for frame in self.frames.iter().rev() {
            if let Some(v) = frame.get(name) {
                return Some(v);
            }
        }
        None
    }

    pub fn with_bindings<I>(&self, bindings: I) -> Self
    where
        I: IntoIterator<Item = (String, Sexp)>,
    {
        let mut child = self.clone();
        child.push();
        for (k, v) in bindings {
            child.define(k, v);
        }
        child
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_walks_chain() {
        let mut env = Env::new();
        env.define("x", Sexp::int(1));
        env.push();
        env.define("y", Sexp::int(2));
        assert_eq!(env.lookup("x"), Some(&Sexp::int(1)));
        assert_eq!(env.lookup("y"), Some(&Sexp::int(2)));
        env.pop();
        assert_eq!(env.lookup("y"), None);
    }

    #[test]
    fn inner_shadows_outer() {
        let mut env = Env::new();
        env.define("x", Sexp::int(1));
        env.push();
        env.define("x", Sexp::int(99));
        assert_eq!(env.lookup("x"), Some(&Sexp::int(99)));
        env.pop();
        assert_eq!(env.lookup("x"), Some(&Sexp::int(1)));
    }
}
