# forged-client

A small, ergonomic Rust client for the Forge gRPC API provided by the `forged` server.

This library wraps the tonic-generated clients and exposes a high-level `ForgedClient` with helpers for:
- Creating repositories and storing versions (package.kdl)
- Pushing raw Git packfiles (SmartPush) with ed25519 proof-based authentication
- Fetching the latest stored packfile (SmartFetch)

Status: early and evolving alongside the server's pure-Rust GitWire implementation.

## Quick start

Add to your `Cargo.toml`:

```toml
[dependencies]
forged-client = { path = "../forged-client" }
```

Connect and use:

```rust,no_run
use forged_client::{ForgedClient, AuthKind};

# #[tokio::main]
# async fn main() -> miette::Result<()> {
let mut client = ForgedClient::connect("http://127.0.0.1:50051").await?;

// Create a repository for a component id
client.create_repo("com.example.zlib").await?;

// Push a prebuilt pack file using an ed25519 key (32-byte seed) and arbitrary proof message
let mut pack = tokio::fs::File::open("./zlib.pack").await?;
let private_key_seed = [0u8; 32]; // load from secure storage
let proof_msg = b"forge-push-proof-v1";
client.push_pack_ed25519(
    "com.example.zlib",
    "alice@example.com",
    AuthKind::User,
    "k1",
    &private_key_seed,
    proof_msg,
    &mut pack,
).await?;

// Fetch the latest pack
let bytes = client.fetch_latest_pack_bytes("com.example.zlib").await?;
println!("received pack ({} bytes)", bytes.len());
# Ok(())
# }
```

## Endpoints covered

- GitService:
  - `CreateRepo`
  - `PutVersion`
  - `SmartPush` (helper: `push_pack_ed25519`)
  - `SmartFetch` (helpers: `fetch_latest_pack_to_writer`, `fetch_latest_pack_bytes`)
- AuthService, GateService, ComponentService clients are exposed directly under `ForgedClient` if you need to use lower-level RPCs.

## Notes

- The development server runs without TLS by default. Use an `http://` endpoint string. For TLS, configure the server and point the client at an `https://` endpoint.
- The SmartPush helper signs a caller-provided `proof_message` using an ed25519 32-byte seed. The server must have the corresponding public key attached to the actor with the given `key_id`.
- For large packs, prefer writing to a file via `fetch_latest_pack_to_writer()` instead of collecting into memory.

## License

MPL-2.0, same as the workspace.
