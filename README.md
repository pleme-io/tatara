# tatara

Nix-native workload orchestrator.

## Overview

Tatara is a distributed workload orchestrator that evaluates Nix flake outputs to define and schedule jobs across a cluster of nodes. It uses Raft consensus (openraft) for leader election, gossip protocol (chitchat) for membership and state propagation, mDNS for bootstrap discovery, and a persistent log store (redb). It provides a CLI, a REST/GraphQL API server, and a TUI for cluster monitoring.

## Usage

```bash
# Build
nix build

# CLI commands
tatara job list                    # List all jobs
tatara job run <file> [--eval]     # Submit a job
tatara job stop <id>               # Stop a job
tatara node list                   # List cluster nodes
tatara node drain <id>             # Drain a node
tatara source add <name> <ref>     # Register a flake source
tatara source sync <name>          # Force reconciliation
tatara alloc list                  # List allocations
tatara context set <endpoint>      # Set API endpoint

# Run as server
tatara server
```

## Architecture

- **Cluster**: Raft consensus + gossip protocol + mDNS discovery
- **Drivers**: Nix-native process execution
- **Sources**: Git-based flake source reconciliation
- **Forges**: Job spec generation patterns (via forgeworks)
- **API**: REST + GraphQL + optional gRPC

## Project Structure

```
src/
  cli/       -- CLI commands (job, node, alloc, source, context, forge)
  cluster/   -- Raft, gossip, p2p networking
  domain/    -- Core domain types (jobs, allocations, nodes)
  drivers/   -- Workload execution drivers
  api/       -- REST and GraphQL server
  nix_eval/  -- Nix flake evaluation
  grpc/      -- Optional gRPC service
```

## License

MIT
