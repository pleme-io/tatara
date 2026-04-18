//! Scoped environment — parent-pointer tree of bindings.
//!
//! Cheap to clone (Arc-backed). Extension is non-destructive: `extend` returns
//! a new `Env` whose parent is `self`. That keeps closures immutable and
//! removes a class of aliasing bugs that plague mutable interpreters.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::value::Value;

#[derive(Clone, Default)]
pub struct Env {
    inner: Arc<EnvFrame>,
}

struct EnvFrame {
    bindings: BTreeMap<String, Value>,
    parent: Option<Arc<EnvFrame>>,
}

impl Default for EnvFrame {
    fn default() -> Self {
        Self {
            bindings: BTreeMap::new(),
            parent: None,
        }
    }
}

impl Env {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lookup(&self, name: &str) -> Option<Value> {
        let mut cursor = Some(&self.inner);
        while let Some(frame) = cursor {
            if let Some(v) = frame.bindings.get(name) {
                return Some(v.clone());
            }
            cursor = frame.parent.as_ref();
        }
        None
    }

    /// Add one binding on top. Returns a new Env (immutable API).
    pub fn extend(&self, name: impl Into<String>, value: Value) -> Self {
        let mut bindings = BTreeMap::new();
        bindings.insert(name.into(), value);
        Self {
            inner: Arc::new(EnvFrame {
                bindings,
                parent: Some(self.inner.clone()),
            }),
        }
    }

    /// Add many bindings on top in one frame.
    pub fn extend_many<I, K, V>(&self, it: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<Value>,
    {
        let mut bindings = BTreeMap::new();
        for (k, v) in it {
            bindings.insert(k.into(), v.into());
        }
        Self {
            inner: Arc::new(EnvFrame {
                bindings,
                parent: Some(self.inner.clone()),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_walks_parent_chain() {
        let e0 = Env::new().extend("x", Value::Int(1));
        let e1 = e0.extend("y", Value::Int(2));
        assert!(matches!(e1.lookup("x"), Some(Value::Int(1))));
        assert!(matches!(e1.lookup("y"), Some(Value::Int(2))));
        assert!(e1.lookup("z").is_none());
    }

    #[test]
    fn extend_is_non_destructive() {
        let e0 = Env::new().extend("x", Value::Int(1));
        let _e1 = e0.extend("x", Value::Int(2));
        // e0 still sees the original binding
        assert!(matches!(e0.lookup("x"), Some(Value::Int(1))));
    }
}
