# gRPC API

Forge exposes its functionality through gRPC services defined in `crates/forged/proto/api_v2.proto`. This is the primary API used by `pkgdev` and other clients.

## Services

### AuthService

Handles actor registration and authentication.

| RPC | Description |
|---|---|
| `Authenticate` | Validate an OIDC token and return an actor reference |
| `RegisterActor` | Register a new actor with display name, email, and Ed25519 public key |
| `RegistrationConfirmation` | Confirm registration by decrypting a challenge envelope |
| `AddActorKey` | Add a new public key to an existing actor |

### GateService

Manages gates and their members.

| RPC | Description |
|---|---|
| `CreateGate` | Create a new gate with a KDL definition |
| `GetGate` | Retrieve a gate by ID |
| `UpdateGate` | Update a gate's KDL definition |
| `ListGates` | List all gates, optionally filtered by owner |
| `AddMember` | Add a member to a gate with roles and permissions |
| `RemoveMember` | Remove a member from a gate |
| `ListMembers` | List all members of a gate |
| `ListComponents` | List all components in a gate |

### ComponentService

Manages components and their files.

| RPC | Description |
|---|---|
| `CreateComponent` | Create a new component in a gate |
| `GetComponent` | Retrieve a component by ID |
| `UpdateComponent` | Update a component's KDL recipe |
| `UploadSourceArchive` | Stream-upload a source archive (chunked) |
| `UploadComponentFile` | Stream-upload a component file (patch, license, script) |
| `ListSourceArchives` | List source archives for a component |
| `ListComponentFiles` | List component files, optionally filtered by kind |

### BuildService

Manages build jobs.

| RPC | Description |
|---|---|
| `GetBuildManifest` | Get the full build manifest for a component |
| `DownloadBlob` | Stream-download a blob by hash |
| `SubmitBuild` | Submit a build job for a component |
| `GetBuildStatus` | Check the status of a build job |
| `ListBuilds` | List builds for a component |
| `CancelBuild` | Cancel a running build |

## Key Message Types

### ActorRef

```protobuf
message ActorRef {
  string id = 1;
  string kind = 2;  // "user" or "service"
}
```

### GateInfo

```protobuf
message GateInfo {
  string id = 1;
  string name = 2;
  string gate_kdl = 3;
  string owner_id = 4;
  // timestamps
}
```

### ComponentInfo

```protobuf
message ComponentInfo {
  string id = 1;
  string gate_id = 2;
  string name = 3;
  string recipe_kdl = 4;
  // timestamps
}
```

### BuildManifest

```protobuf
message BuildManifest {
  string component_id = 1;
  string name = 2;
  string recipe_kdl = 3;
  repeated SourceArchiveInfo source_archives = 4;
  repeated ComponentFileInfo patches = 5;
  repeated ComponentFileInfo licenses = 6;
  repeated ComponentFileInfo scripts = 7;
}
```

### BuildJobInfo

```protobuf
message BuildJobInfo {
  string id = 1;
  string component_id = 2;
  string gate_id = 3;
  string actor_id = 4;
  string status = 5;      // "pending", "running", "success", "failed", "cancelled"
  int32 exit_code = 6;
  string summary = 7;
  string build_log_url = 8;
  // timestamps
}
```

## Authentication

Most RPCs require an authenticated actor. The client sends credentials via gRPC metadata. The server's Tower auth middleware validates the token and injects the actor identity into the request context.

## Connecting

```bash
# Default server address
grpcurl -plaintext localhost:50051 list
```

The `forged-client` crate provides a Rust client library that wraps the generated protobuf types.
