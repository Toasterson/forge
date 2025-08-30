# Project Guidelines for Junie

This document provides guidelines for Junie (JetBrains AI Coding Agent) when working on the forge Rust codebase.

### Dependencies
The project uses several key dependencies:
- **Error handling**: thiserror and miette
- **Serialization**: serde and serde_json
- **Logging**: tracing and tracing-subscriber
- **Configuration**: config
- **DSL for Builds** kdl
- **Compression**: flate2 and lz4
- **Versioning**: semver
- **CLI**: clap

### Dependencies Setup

**For Application Crates:**
```toml
[dependencies]
miette = { version = "7.6.0", features = ["fancy"] }
thiserror = "1.0.50"
tracing = "0.1.37"
tracing-subscriber = "0.3.17"
```

**For Library Crates:**
```toml
[dependencies]
miette = "7.6.0"
thiserror = "1.0.50"
tracing = "0.1.37"
```

**Rule:** Only enable the "fancy" feature in top-level application crates, not in library crates.

### Error Type Definition

Define error types as enums using thiserror and miette's Diagnostic derive macro:

```rust
use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
#[error("A validation error occurred")]
#[diagnostic(
    code(ips::validation_error),
    help("Please check the input data and try again.")
)]
pub enum ValidationError {
    // Error variants go here
}
```

### Error Code Naming Convention

Use a consistent naming scheme for error codes:
- Top-level errors: `ips::category_error` (e.g., `ips::validation_error`)
- Specific errors: `ips::category_error::specific_error` (e.g., `ips::validation_error::invalid_name`)

Examples:
- `ips::validation_error`
- `ips::validation_error::invalid_name`

### Error Handling in Library vs. Application Code

- In library code (like `libips`), always return specific error types
- In application code, you can use `miette::Result` for convenience

### Decision Tree for Error Handling

1. **Is this a library or application crate?**
   - **Library**: Use specific error types, don't use miette's "fancy" feature
   - **Application**: Can use miette::Result for convenience, enable "fancy" feature

2. **What type of error is being handled?**
   - **Input validation**: Create specific error variants with helpful messages
   - **I/O operations**: Wrap std::io::Error with transparent error
   - **Parsing**: Include source highlighting with NamedSource and SourceSpan
   - **External library errors**: Wrap with transparent error or convert with From

3. **How should the error be propagated?**
   - **Within same error type**: Use ? operator
   - **Between different error types**: Use map_err or implement From trait

4. **What level of diagnostic information is needed?**
   - **Basic**: Just use #[error] attribute
   - **Medium**: Add #[diagnostic] with code and help
   - **Detailed**: Include source_code, label, and related information

For more detailed guidelines on error handling, refer to:
- `./doc/rust_docs/error_handling.md`

## Testing Guidelines

### Running Tests

To run tests for the entire project:
```bash
cargo test
```

To run tests for a specific crate:
```bash
cargo test -p <crate_name>
```

### Writing Tests

- Unit tests should be placed in the same file as the code they're testing, in a `mod tests` block
- Integration tests should be placed in the `tests` directory of each crate
- End-to-end tests should use the test environment set up by `cargo xtask setup-test-env`

## Build Guidelines

### Building the Project

Using cargo directly:
```bash
cargo build                    # Build the entire project
cargo build -p <crate_name>    # Build a specific crate
cargo build --release          # Build with optimizations for release
```

Using cargo-xtask:
```bash
cargo xtask build              # Build the entire project
cargo xtask build -p <crate_name>  # Build a specific crate
cargo xtask build -r           # Build with optimizations for release
```

This order is important as it reflects the dependency hierarchy, with `libips` being the foundation that other crates build upon.

### Adding New Commands

To add a new command to cargo-xtask:

1. Edit the `xtask/src/main.rs` file
2. Add a new variant to the `Commands` enum
3. Implement a function for the new command
4. Add a match arm in the `main` function to call your new function

## Code Style Guidelines

### General Guidelines

- Follow the Rust standard style guide
- Use `cargo fmt` to format code
- Use `cargo clippy` to check for common mistakes and improve code quality

### Naming Conventions

- Use snake_case for variables, functions, and modules
- Use CamelCase for types, traits, and enums
- Use SCREAMING_SNAKE_CASE for constants
- Prefix unsafe functions with `unsafe_`

### Documentation

- Document all public items with doc comments
- Include examples in doc comments where appropriate
- Document error conditions and return values

### Error Handling

- Follow the error handling guidelines above
- Use the ? operator for error propagation where appropriate
- Avoid using `unwrap()` or `expect()` in production code

### Logging

- Use the tracing crate for logging
- Use appropriate log levels:
  - `trace`: Very detailed information
  - `debug`: Useful information for debugging
  - `info`: General information about the application's operation
  - `warn`: Potentially problematic situations
  - `error`: Error conditions

## Workflow for Junie

When working on the IPS project, Junie should follow this workflow:

1. **Understand the Issue**: Thoroughly review the issue description and related code
2. **Plan the Changes**: Create a plan for implementing the changes
3. **Implement the Changes**: Make the necessary changes to the code
4. **Test the Changes**: Run tests to ensure the changes work as expected
5. **Document the Changes**: Update documentation as needed
6. **Submit the Changes**: Submit the changes for review

When implementing error handling, Junie should follow the error handling guidelines above and use the decision tree to determine the appropriate approach.