// Phase 2: Server↔Client internal gRPC protocol.
//
// Will implement:
// - Node registration
// - Bidirectional heartbeat (client sends status, server pushes allocations)
// - Allocation start/stop RPCs
// - Task log streaming from client to server
//
// For Phase 1 (embedded mode), these are stubs.

#[cfg(feature = "grpc")]
use super::proto::*;
