//! Non-aarch64-darwin stub. HVF doesn't exist on Linux / Intel Mac, so
//! `HvfEngine::new()` returns a typed error. The struct still exists so
//! downstream crates compile.

#![cfg(not(all(target_arch = "aarch64", target_os = "macos")))]

use crate::HvfError;

pub struct HvfEngine;

impl HvfEngine {
    /// Stub on non-Apple-Silicon targets — always returns
    /// `HvfError::UnsupportedPlatform`.
    ///
    /// # Errors
    /// Always.
    pub fn new() -> Result<Self, HvfError> {
        Err(HvfError::UnsupportedPlatform)
    }
}
