// Re-export protobuf modules from the shared `forged-client` crate.
// Package: forged.api.v1
pub mod forged {
    pub mod api {
        pub use forged_client::api::forged::api::v1;
        pub use forged_client::api::forged::api::v2;
    }
}
