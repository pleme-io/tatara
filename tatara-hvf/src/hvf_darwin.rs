//! Apple Silicon Hypervisor.framework backend. Only compiled on
//! `aarch64-darwin`.
//!
//! Wraps the `applevisor` safe-wrapper crate (Quarkslab 1.0.0). This
//! module owns the lifecycle primitives — VM, vCPU, memory — that
//! Phase H.2.2's virtio device implementations will build on.
//!
//! API map (applevisor 1.0.0):
//!
//! - `VirtualMachine::new() -> Result<VirtualMachineInstance<GicDisabled>>`
//! - `VirtualMachineInstance::vcpu_create() -> Result<Vcpu>`
//! - `VirtualMachineInstance::memory_create(size) -> Result<Memory>`
//! - `Memory::map(guest_addr, perms)`
//! - `Memory::write(guest_addr, &[u8])` / `Memory::read(guest_addr, &mut [u8])`
//! - `Vcpu::get_reg(Reg) / set_reg(Reg, value)` / `run()`

#![cfg(all(target_arch = "aarch64", target_os = "macos"))]

use applevisor::prelude::{
    ExitReason, GicDisabled, MemPerms, Memory, Reg, SysReg, Vcpu, VirtualMachine,
    VirtualMachineInstance,
};

use crate::{GuestRegion, HvfError, Permissions};

/// HVF-backed virtual machine wrapper.
///
/// Holds a `VirtualMachineInstance`, the vCPUs it created, and the
/// memory regions it owns. applevisor's internal `Arc` guards keep
/// drop ordering safe across these collections.
pub struct HvfEngine {
    vm: VirtualMachineInstance<GicDisabled>,
    memories: Vec<Memory>,
    vcpus: Vec<Vcpu>,
}

impl HvfEngine {
    /// Initialize a fresh HVF-backed VM. Requires the
    /// `com.apple.security.hypervisor` entitlement on the process.
    ///
    /// # Errors
    /// Returns `HvfError::MissingEntitlement` or `HvfError::VmCreate`
    /// depending on why `hv_vm_create` fails.
    pub fn new() -> Result<Self, HvfError> {
        let vm = VirtualMachine::new().map_err(|e| map_error(e, HvfError::VmCreate))?;
        Ok(Self {
            vm,
            memories: Vec::new(),
            vcpus: Vec::new(),
        })
    }

    /// Allocate a guest memory region of `size` bytes and map it at
    /// `guest_phys`. Returns a `GuestRegion` handle; the underlying
    /// `Memory` is retained by the engine.
    ///
    /// # Errors
    /// Returns `HvfError::MemoryMap` on allocation or map failure.
    pub fn create_memory(
        &mut self,
        guest_phys: u64,
        size: usize,
        perms: Permissions,
    ) -> Result<GuestRegion, HvfError> {
        let mut mem = self
            .vm
            .memory_create(size)
            .map_err(|e| map_error(e, HvfError::MemoryMap))?;
        mem.map(guest_phys, perms_to_memperms(perms))
            .map_err(|e| map_error(e, HvfError::MemoryMap))?;
        self.memories.push(mem);
        Ok(GuestRegion {
            guest_phys_addr: guest_phys,
            size_bytes: size,
            permissions: perms,
        })
    }

    /// Write `bytes` into a previously-created region at `guest_phys`.
    ///
    /// # Errors
    /// Returns `HvfError::MemoryMap` if no region covers the address
    /// or the underlying write fails.
    pub fn write_guest_bytes(&mut self, guest_phys: u64, bytes: &[u8]) -> Result<(), HvfError> {
        let mem = self
            .memories
            .iter_mut()
            .find(|m| {
                m.guest_addr()
                    .map(|base| {
                        guest_phys >= base
                            && guest_phys + bytes.len() as u64 <= base + m.size() as u64
                    })
                    .unwrap_or(false)
            })
            .ok_or_else(|| {
                HvfError::MemoryMap(format!(
                    "no region covers guest_phys=0x{guest_phys:x} (len={})",
                    bytes.len()
                ))
            })?;
        mem.write(guest_phys, bytes)
            .map_err(|e| map_error(e, HvfError::MemoryMap))
    }

    /// Create a vCPU. Returns its index.
    ///
    /// # Errors
    /// Returns `HvfError::VCpuCreate` on `hv_vcpu_create` failure.
    pub fn create_vcpu(&mut self) -> Result<usize, HvfError> {
        let vcpu = self
            .vm
            .vcpu_create()
            .map_err(|e| map_error(e, HvfError::VCpuCreate))?;
        self.vcpus.push(vcpu);
        Ok(self.vcpus.len() - 1)
    }

    /// Read a general-purpose register from a vCPU.
    ///
    /// # Errors
    /// Returns `HvfError::Register` on access failure.
    pub fn vcpu_read_reg(&self, vcpu_idx: usize, reg: Reg) -> Result<u64, HvfError> {
        let vcpu = self.vcpu(vcpu_idx)?;
        vcpu.get_reg(reg)
            .map_err(|e| HvfError::Register(format!("{e:?}")))
    }

    /// Write a general-purpose register on a vCPU.
    ///
    /// # Errors
    /// Returns `HvfError::Register` on access failure.
    pub fn vcpu_write_reg(&self, vcpu_idx: usize, reg: Reg, value: u64) -> Result<(), HvfError> {
        let vcpu = self.vcpu(vcpu_idx)?;
        vcpu.set_reg(reg, value)
            .map_err(|e| HvfError::Register(format!("{e:?}")))
    }

    /// Run a vCPU. Blocks until the vCPU exits.
    ///
    /// # Errors
    /// Returns `HvfError::VCpuRun` on failure.
    pub fn vcpu_run(&self, vcpu_idx: usize) -> Result<(), HvfError> {
        let vcpu = self.vcpu(vcpu_idx)?;
        vcpu.run().map_err(|e| map_error(e, HvfError::VCpuRun))
    }

    /// Read a system register (SCTLR_EL1, CPACR_EL1, SP_EL1, etc.).
    ///
    /// # Errors
    /// Returns `HvfError::Register` on access failure.
    pub fn vcpu_read_sys_reg(&self, vcpu_idx: usize, reg: SysReg) -> Result<u64, HvfError> {
        let vcpu = self.vcpu(vcpu_idx)?;
        vcpu.get_sys_reg(reg)
            .map_err(|e| HvfError::Register(format!("{e:?}")))
    }

    /// Write a system register. Used to set up the vCPU's initial
    /// architectural state (PSTATE via CPSR, SCTLR_EL1, SP_EL1, etc.)
    /// before the first `vcpu_run`.
    ///
    /// # Errors
    /// Returns `HvfError::Register` on access failure.
    pub fn vcpu_write_sys_reg(
        &self,
        vcpu_idx: usize,
        reg: SysReg,
        value: u64,
    ) -> Result<(), HvfError> {
        let vcpu = self.vcpu(vcpu_idx)?;
        vcpu.set_sys_reg(reg, value)
            .map_err(|e| HvfError::Register(format!("{e:?}")))
    }

    /// Inspect why the vCPU last exited.
    ///
    /// # Errors
    /// Returns `HvfError::Register` when the index is out of range.
    pub fn vcpu_exit_reason(&self, vcpu_idx: usize) -> Result<ExitReason, HvfError> {
        let vcpu = self.vcpu(vcpu_idx)?;
        Ok(vcpu.get_exit_info().reason)
    }

    #[must_use]
    pub fn vcpu_count(&self) -> usize {
        self.vcpus.len()
    }

    #[must_use]
    pub fn memory_region_count(&self) -> usize {
        self.memories.len()
    }

    fn vcpu(&self, idx: usize) -> Result<&Vcpu, HvfError> {
        self.vcpus
            .get(idx)
            .ok_or_else(|| HvfError::Register(format!("vcpu index {idx} out of range")))
    }
}

fn perms_to_memperms(p: Permissions) -> MemPerms {
    match (p.read, p.write, p.execute) {
        (true, true, true) => MemPerms::RWX,
        (true, true, false) => MemPerms::RW,
        (true, false, true) => MemPerms::RX,
        (true, false, false) => MemPerms::R,
        _ => MemPerms::RW,
    }
}

fn map_error<E: std::fmt::Debug>(e: E, wrap: impl FnOnce(String) -> HvfError) -> HvfError {
    let msg = format!("{e:?}");
    if msg.contains("HV_ERROR") || msg.to_lowercase().contains("entitle") {
        HvfError::MissingEntitlement
    } else {
        wrap(msg)
    }
}
