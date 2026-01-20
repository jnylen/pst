use crate::models::{ProgressTracker, UploadRequest, UploadResponse, UploadType};
use crate::providers::{ProviderCapabilities, UploadError, UploadService};
use async_trait::async_trait;

pub struct ZeroX0STProvider {
    endpoint: String,
    timeout_seconds: u64,
}

impl ZeroX0STProvider {
    pub fn new(timeout_seconds: u64) -> Self {
        Self {
            endpoint: "https://0x0.st".to_string(),
            timeout_seconds,
        }
    }
}

#[async_trait]
impl UploadService for ZeroX0STProvider {
    fn provider_name(&self) -> &str {
        "0x0st"
    }

    fn supports_upload_type(&self, upload_type: UploadType) -> bool {
        matches!(upload_type, UploadType::File | UploadType::Image)
    }

    fn max_file_size(&self) -> u64 {
        512 * 1024 * 1024 // 512 MiB
    }

    async fn upload(
        &self,
        request: &UploadRequest,
        _progress: Option<&ProgressTracker>,
    ) -> Result<UploadResponse, UploadError> {
        let content_size = request.content.len() as u64;

        if content_size > self.max_file_size() {
            return Err(UploadError::FileTooLarge {
                max_size: self.max_file_size(),
                actual_size: content_size,
            });
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(self.timeout_seconds))
            .build()
            .map_err(|e| UploadError::ConnectionFailed(e.to_string()))?;

        let filename = request
            .filename
            .clone()
            .unwrap_or_else(|| "file".to_string());

        let mime_type = request
            .filename
            .as_ref()
            .and_then(|name| {
                std::path::Path::new(name)
                    .extension()
                    .and_then(|ext| ext.to_str())
            })
            .map(|ext| match ext.to_lowercase().as_str() {
                "txt" | "log" | "md" => "text/plain",
                "html" | "htm" => "text/html",
                "css" => "text/css",
                "js" => "application/javascript",
                "json" => "application/json",
                "xml" => "application/xml",
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "webp" => "image/webp",
                "svg" => "image/svg+xml",
                "pdf" => "application/pdf",
                "zip" => "application/zip",
                _ => "application/octet-stream",
            })
            .unwrap_or("application/octet-stream");

        let form = reqwest::multipart::Form::new().part(
            "file",
            reqwest::multipart::Part::bytes(request.content.clone())
                .file_name(filename)
                .mime_str(mime_type)
                .map_err(|e| UploadError::UploadFailed(e.to_string()))?,
        );

        let response = client
            .post(&self.endpoint)
            .header("User-Agent", format!("pst/{}", env!("CARGO_PKG_VERSION")))
            .multipart(form)
            .send()
            .await
            .map_err(|e| UploadError::ConnectionFailed(e.to_string()))?;

        let status = response.status();

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(UploadError::UploadFailed(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        let url = response
            .text()
            .await
            .map_err(|e| UploadError::InvalidResponse(e.to_string()))?;

        let url = url.trim().to_string();

        if url.is_empty() {
            return Err(UploadError::InvalidResponse(
                "Empty response from server".to_string(),
            ));
        }

        Ok(UploadResponse::success(
            url,
            self.provider_name().to_string(),
            None,
        ))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_expiration: true,
            supports_custom_names: true,
            requires_auth: false,
            supports_direct_text: false,
        }
    }
}
