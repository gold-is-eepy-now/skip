# skype-rs

`skype-rs` is a production-oriented Rust workspace that emulates the *infrastructure* ideas of legacy Skype's hybrid architecture.

It intentionally excludes any proprietary Skype clients or binaries and focuses only on self-hosted network services:

- **login-server**: authentication, sessions, and supernode bootstrap
- **supernode**: peer directory, signaling routing, and presence tracking
- **relay-node**: UDP packet relay fallback for NAT-blocked peers

> Compatibility note: this project does **not** implement Skype's original proprietary wire protocol and therefore does **not** interoperate with stock Skype clients (including Skype 4.2).

## Build

```bash
cargo build
```

## Run

```bash
cargo run -p login-server --bin login-server
cargo run -p supernode --bin supernode
cargo run -p relay-node --bin relay-node
```

## Protocol flow for custom peers

1. Connect to `login-server`, send `register` and/or `login`.
2. Use returned session token to connect to `supernode`.
3. Send `announce_peer` with your TCP/UDP endpoints.
4. Send periodic `heartbeat` (<= 90s).
5. Use `peer_lookup` / `connect_request` and `signal_forward` for signaling.
6. If `relay_required`, send UDP relay packets to `relay-node`.

## Configuration

The shared network configuration file lives at `config/network.toml`.
Set `database_url` there (default: `sqlite://./data/skype-rs.db`). The services will
create parent directories and the SQLite file automatically when possible.

## Workspace layout

```text
crates/
  common/       # shared config, errors, utility types
  protocol/     # JSON protocol messages and wire helpers
  database/     # SQLite + SQLx persistence (users/sessions/contacts)
  login-server/ # login and bootstrap service over TCP
  supernode/    # peer directory + signaling router + heartbeats
  relay-node/   # UDP relay transport for simulated voice/data packets
tools/
  dev-supernode-cluster.rs
```

## Notes

- TCP is used for auth/signaling/presence.
- UDP is used for packet relay simulation.
- Supernodes maintain local routing tables and a cache of known supernodes.
- The design keeps extension points for gossip, STUN-like NAT traversal, election, and federation.
