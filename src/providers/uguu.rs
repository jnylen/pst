use crate::models::{ProgressTracker, UploadRequest, UploadResponse, UploadType};
use crate::providers::{UploadError, UploadService};
use async_trait::async_trait;
use serde::Deserialize;

#[derive(Deserialize)]
struct UguuResponse {
    files: Vec<UguuFile>,
}

#[derive(Deserialize)]
struct UguuFile {
    url: String,
    #[allow(dead_code)]
    #[serde(rename = "filename")]
    name: String,
}

pub struct UguuProvider {
    endpoint: String,
    timeout_seconds: u64,
}

impl UguuProvider {
    pub fn new(timeout_seconds: u64) -> Self {
        Self {
            endpoint: "https://uguu.se/upload".to_string(),
            timeout_seconds,
        }
    }
}

#[async_trait]
impl UploadService for UguuProvider {
    fn provider_name(&self) -> &str {
        "uguu"
    }

    fn supports_upload_type(&self, upload_type: UploadType) -> bool {
        matches!(
            upload_type,
            UploadType::File | UploadType::Image | UploadType::Paste
        )
    }

    fn max_file_size(&self) -> u64 {
        128 * 1024 * 1024 // 128 MiB
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
            .user_agent(format!("pst/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| UploadError::ConnectionFailed(e.to_string()))?;

        let filename = request
            .filename
            .clone()
            .unwrap_or_else(|| "file".to_string());

        // Determine mime type from filename extension
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
            "files[]",
            reqwest::multipart::Part::bytes(request.content.clone())
                .file_name(filename)
                .mime_str(mime_type)
                .map_err(|e| UploadError::UploadFailed(e.to_string()))?,
        );

        let response = client
            .post(&self.endpoint)
            .query(&[("output", "json")])
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

        let response_text = response
            .text()
            .await
            .map_err(|e| UploadError::InvalidResponse(e.to_string()))?;

        let uguu_response: UguuResponse = serde_json::from_str(&response_text)
            .map_err(|e| UploadError::InvalidResponse(e.to_string()))?;

        if uguu_response.files.is_empty() {
            return Err(UploadError::InvalidResponse(
                "No files in response".to_string(),
            ));
        }

        let url = uguu_response.files[0].url.clone();

        Ok(UploadResponse::success(
            url,
            self.provider_name().to_string(),
            None,
        ))
    }
}
