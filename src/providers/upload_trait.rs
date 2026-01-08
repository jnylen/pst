use crate::models::{ProgressTracker, UploadRequest, UploadResponse, UploadType};
use async_trait::async_trait;

#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Upload failed: {0}")]
    UploadFailed(String),

    #[error("HTTP error {status_code}: {message}")]
    #[allow(dead_code)]
    HttpError { status_code: u16, message: String },

    #[error("File too large: max {max_size} bytes, got {actual_size} bytes")]
    FileTooLarge { max_size: u64, actual_size: u64 },

    #[error("Rate limited: retry after {retry_after}s")]
    #[allow(dead_code)]
    RateLimited { retry_after: u64 },

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    #[error("Provider not available: {0}")]
    ProviderNotAvailable(String),

    #[error("Timeout: {0}")]
    #[allow(dead_code)]
    Timeout(String),
}

#[async_trait]
pub trait UploadService: Send + Sync {
    fn provider_name(&self) -> &str;

    fn supports_upload_type(&self, upload_type: UploadType) -> bool;

    fn max_file_size(&self) -> u64;

    async fn upload(
        &self,
        request: &UploadRequest,
        progress: Option<&ProgressTracker>,
    ) -> Result<UploadResponse, UploadError>;

    async fn test_connection(&self) -> bool {
        true
    }

    #[allow(dead_code)]
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_expiration: false,
            supports_custom_names: false,
            requires_auth: false,
            supports_direct_text: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct ProviderCapabilities {
    pub supports_expiration: bool,
    pub supports_custom_names: bool,
    pub requires_auth: bool,
    pub supports_direct_text: bool,
}
