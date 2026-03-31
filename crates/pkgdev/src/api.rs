// Re-export protobuf modules from the shared `forged-client` crate.
pub mod forged {
    pub mod api {
        pub use forged_client::api::forged::api::v2;
    }
}
