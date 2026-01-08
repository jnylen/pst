use crate::models::{ProgressTracker, UploadRequest, UploadResponse, UploadType};
use crate::providers::{UploadError, UploadService};
use async_trait::async_trait;

pub struct PasteRsProvider {
    endpoint: String,
    timeout_seconds: u64,
}

impl PasteRsProvider {
    pub fn new(timeout_seconds: u64) -> Self {
        Self {
            endpoint: "https://paste.rs".to_string(),
            timeout_seconds,
        }
    }
}

#[async_trait]
impl UploadService for PasteRsProvider {
    fn provider_name(&self) -> &str {
        "paste_rs"
    }

    fn supports_upload_type(&self, upload_type: UploadType) -> bool {
        matches!(upload_type, UploadType::Paste)
    }

    fn max_file_size(&self) -> u64 {
        10 * 1024 * 1024 // 10 MiB (conservative estimate)
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

        let response = client
            .post(&self.endpoint)
            .body(request.content.clone())
            .send()
            .await
            .map_err(|e| UploadError::ConnectionFailed(e.to_string()))?;

        let status = response.status();

        if status != 201 && status != 206 {
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

        let url = if url.starts_with("http") {
            url
        } else {
            format!("https://paste.rs/{}", url)
        };

        Ok(UploadResponse::success(
            url,
            self.provider_name().to_string(),
            None,
        ))
    }
}
