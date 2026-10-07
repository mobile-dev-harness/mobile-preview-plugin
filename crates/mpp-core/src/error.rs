#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid argument: {message}")]
    InvalidArgument { message: String },
    #[error("not found: {what}")]
    NotFound { what: String },
    #[error("device {device} is reserved by {owner}")]
    Busy { device: String, owner: String },
    #[error("the device session is no longer current")]
    StaleSession,
    #[error("permission denied: {message}")]
    PermissionDenied { message: String },
    #[error("unsupported feature: {feature}")]
    Unsupported { feature: String },
    #[error("tool not found: {tool}")]
    ToolNotFound { tool: String },
    #[error("{tool} failed: {message}")]
    CommandFailed { tool: String, message: String },
    #[error("operation timed out: {operation}")]
    Timeout { operation: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidArgument { .. } => "INVALID_ARGUMENT",
            Self::NotFound { .. } => "NOT_FOUND",
            Self::Busy { .. } => "BUSY",
            Self::StaleSession => "STALE_SESSION",
            Self::PermissionDenied { .. } => "PERMISSION_DENIED",
            Self::Unsupported { .. } => "UNSUPPORTED",
            Self::ToolNotFound { .. } => "TOOL_NOT_FOUND",
            Self::CommandFailed { .. } => "COMMAND_FAILED",
            Self::Timeout { .. } => "TIMEOUT",
            Self::Io(_) => "IO_ERROR",
        }
    }

    pub fn hint(&self) -> String {
        match self {
            Self::InvalidArgument { .. } => "Correct the request fields and retry.",
            Self::NotFound { .. } => "Refresh the device inventory and select an available device.",
            Self::Busy { .. } => "Disconnect the existing device session before connecting again.",
            Self::StaleSession => {
                "Connect again and use the newly returned session ID and generation."
            }
            Self::PermissionDenied { .. } => "Authorize device access and retry the operation.",
            Self::Unsupported { .. } => {
                "Check the device capabilities and choose a supported operation."
            }
            Self::ToolNotFound { .. } => {
                "Install the required tool or configure its executable path."
            }
            Self::CommandFailed { .. } => {
                "Check the tool diagnostic and device connection before retrying."
            }
            Self::Timeout { .. } => "Check that the device and tool are responsive, then retry.",
            Self::Io(_) => "Check local paths, permissions, and connections before retrying.",
        }
        .to_owned()
    }
}
