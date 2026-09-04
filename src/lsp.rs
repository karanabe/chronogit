//! Optional, profile-driven Language Server Protocol client support.
//!
//! ChronoGit does not bundle or download language servers. A client is created
//! only for profiles explicitly enabled by the user, and every process is
//! started directly with an argument vector rather than through a shell.

mod config;
mod manager;
mod position;
mod protocol;
mod session;

use std::error::Error;
use std::fmt::{self, Display, Formatter};

pub use config::{LspConfig, LspConfigError, ServerProfile};
pub use manager::LspManager;
pub(crate) use manager::WireNavigationTarget;
pub(crate) use position::from_lsp_character;
pub use position::{PositionEncoding, display_column, next_byte_column, previous_byte_column};

/// A recoverable failure at the optional language-server boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LspError {
    /// No profile was explicitly enabled for the current file.
    Disabled(String),
    /// More than one enabled profile claims the current extension.
    AmbiguousProfile(String),
    /// The document cannot safely or completely be synchronized.
    InvalidDocument(String),
    /// The configured server could not be started or communicated with.
    Process(String),
    /// The peer sent invalid or unsupported protocol data.
    Protocol(String),
    /// The server returned a JSON-RPC error for a valid request.
    RequestFailed(String),
    /// The server invalidated a request while its internal document changed.
    ContentModified,
    /// A bounded operation exceeded its deadline.
    Timeout(String),
    /// The initialized server does not advertise the requested operation.
    Unsupported(String),
}

impl Display for LspError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled(detail)
            | Self::AmbiguousProfile(detail)
            | Self::InvalidDocument(detail)
            | Self::Process(detail)
            | Self::Protocol(detail)
            | Self::RequestFailed(detail)
            | Self::Timeout(detail)
            | Self::Unsupported(detail) => formatter.write_str(detail),
            Self::ContentModified => {
                formatter.write_str("language server content changed while resolving the request")
            }
        }
    }
}

impl Error for LspError {}
