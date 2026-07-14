//! URI resolution and `$ref` handling for Schemaforge.
//!
//! The default [`OfflineResolver`] resolves schemas from an in-memory registry
//! only.  [`FileResolver`] additionally loads schemas from the filesystem and
//! is ready for future compiler wiring.  [`NetworkResolver`] always denies
//! network access (policy: network=deny).  All three implement the [`Resolver`]
//! trait used by the CLI lock workflow and future external-ref consumers.
//!
//! A [`LockFile`] (serialised to `schemaforge.lock.toml`) records every
//! resolved external URI so that builds remain reproducible; it is consumed by
//! the CLI.

pub mod file;
pub(crate) mod fragment;
pub mod limit;
pub mod lock;
pub mod network;
pub mod offline;
pub(crate) mod uri;

pub use file::FileResolver;
pub use limit::LimitingResolver;
pub use lock::{LockEntry, LockFile};
pub use network::NetworkResolver;
pub use offline::OfflineResolver;
pub use uri::resolve_uri;

// ── Error type ────────────────────────────────────────────────────────────────

use thiserror::Error;

/// Error returned when a URI cannot be resolved.
#[derive(Debug, Error)]
pub enum ResolveError {
    /// The URI was not found in the resolver's registry.
    #[error("schema not found for URI: {0}")]
    NotFound(String),
    /// Network access was denied by policy.
    #[error(
        "network access denied (policy: network=deny) for URI `{uri}`; \
         add the schema to an offline registry or unlock network access"
    )]
    NetworkDenied {
        /// The URI that triggered the denial.
        uri: String,
    },
    /// The referenced URI could not be parsed.
    #[error("invalid URI reference `{uri}`: {reason}")]
    InvalidUri {
        /// The URI that failed to parse.
        uri: String,
        /// Why the parse failed.
        reason: String,
    },
    /// The schema content could not be parsed as JSON.
    #[error("failed to parse schema at `{uri}`: {reason}")]
    ParseError {
        /// The URI of the schema.
        uri: String,
        /// JSON parse error message.
        reason: String,
    },
    /// IO error reading from the filesystem.
    #[error("IO error loading `{uri}`: {reason}")]
    IoError {
        /// The URI of the schema.
        uri: String,
        /// IO error message.
        reason: String,
    },
    /// The schema document exceeds the configured size limit.
    #[error("schema at `{uri}` exceeds maximum size ({size} > {limit} bytes)")]
    SizeExceeded {
        /// The URI of the oversized schema.
        uri: String,
        /// Actual byte size.
        size: usize,
        /// Configured limit.
        limit: usize,
    },
    /// The schema document exceeds the configured nesting depth limit.
    #[error("schema at `{uri}` exceeds maximum nesting depth ({depth} > {limit})")]
    DepthExceeded {
        /// The URI of the deep schema.
        uri: String,
        /// Observed depth.
        depth: usize,
        /// Configured limit.
        limit: usize,
    },
    /// The resolved path escapes the configured base-directory jail.
    #[error("path `{path}` escapes the resolver base-directory jail")]
    PathEscaped {
        /// The escaped (normalized) path that was rejected.
        path: String,
    },
}

// ── Resolver trait ────────────────────────────────────────────────────────────

/// Resolves a `$ref` URI to a JSON [`serde_json::Value`].
pub trait Resolver: Send + Sync {
    /// Resolve `reference` relative to `base` and return the schema value.
    ///
    /// The `base` is the `$id` or URI of the document currently being compiled.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError`] when the reference cannot be found or parsed.
    fn resolve(&self, base: &str, reference: &str) -> Result<serde_json::Value, ResolveError>;
}
