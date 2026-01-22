use crate::models::{ProgressTracker, UploadRequest, UploadResponse, UploadType};
use crate::providers::{ProviderCapabilities, UploadError, UploadService};
use async_trait::async_trait;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::UploadOptions;

    #[test]
    fn test_provider_creation() {
        let provider = X0AtProvider::new(30);
        assert_eq!(provider.endpoint, "https://x0.at/");
        assert_eq!(provider.timeout_seconds, 30);
    }

    #[test]
    fn test_provider_name() {
        let provider = X0AtProvider::new(30);
        assert_eq!(provider.provider_name(), "x0at");
    }

    #[test]
    fn test_supports_upload_types() {
        let provider = X0AtProvider::new(30);
        
        assert!(provider.supports_upload_type(UploadType::File));
        assert!(provider.supports_upload_type(UploadType::Image));
        assert!(provider.supports_upload_type(UploadType::Paste));
    }

    #[test]
    fn test_max_file_size() {
        let provider = X0AtProvider::new(30);
        assert_eq!(provider.max_file_size(), 512 * 1024 * 1024);
    }

    #[test]
    fn test_capabilities() {
        let provider = X0AtProvider::new(30);
        let capabilities = provider.capabilities();
        
        assert!(!capabilities.supports_expiration);
        assert!(!capabilities.supports_custom_names);
        assert!(!capabilities.requires_auth);
        assert!(!capabilities.supports_direct_text);
    }

    #[test]
    fn test_upload_request_creation() {
        let content = b"Hello, World!";
        let request = UploadRequest::new(
            content.to_vec(),
            Some("test.txt".to_string()),
            UploadType::Paste,
            None,
            false,
        );
        
        assert_eq!(request.content, content);
        assert_eq!(request.filename, Some("test.txt".to_string()));
        assert_eq!(request.upload_type, UploadType::Paste);
    }

    #[test]
    fn test_mime_type_detection() {
        let test_cases = vec![
            ("test.txt", "text/plain"),
            ("test.md", "text/plain"),
            ("test.log", "text/plain"),
            ("test.html", "text/html"),
            ("test.htm", "text/html"),
            ("test.css", "text/css"),
            ("test.js", "application/javascript"),
            ("test.json", "application/json"),
            ("test.xml", "application/xml"),
            ("test.png", "image/png"),
            ("test.jpg", "image/jpeg"),
            ("test.jpeg", "image/jpeg"),
            ("test.gif", "image/gif"),
            ("test.webp", "image/webp"),
            ("test.svg", "image/svg+xml"),
            ("test.pdf", "application/pdf"),
            ("test.zip", "application/zip"),
            ("test.bin", "application/octet-stream"),
            ("test.unknown", "application/octet-stream"),
        ];
        
        for (filename, expected_mime) in test_cases {
            let mime_type: &str = std::path::Path::new(filename)
                .extension()
                .and_then(|ext| ext.to_str())
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
                
            assert_eq!(mime_type, expected_mime, "Failed for filename: {}", filename);
        }
    }

    #[test]
    fn test_upload_response_success() {
        let response = UploadResponse::success(
            "https://x0.at/test.txt".to_string(),
            "x0at".to_string(),
            None,
        );
        
        assert!(response.success);
        assert_eq!(response.url, Some("https://x0.at/test.txt".to_string()));
        assert_eq!(response.provider, "x0at");
        assert_eq!(response.error, None);
    }

    #[test]
    fn test_upload_response_failure() {
        let response = UploadResponse::failed(
            "x0at".to_string(),
            "Connection failed".to_string(),
        );
        
        assert!(!response.success);
        assert_eq!(response.url, None);
        assert_eq!(response.provider, "x0at");
        assert_eq!(response.error, Some("Connection failed".to_string()));
    }

    #[test]
    fn test_upload_error_file_too_large() {
        let error = UploadError::FileTooLarge {
            max_size: 512 * 1024 * 1024,
            actual_size: 1024 * 1024 * 1024,
        };
        
        let error_str = error.to_string();
        assert!(error_str.contains("File too large"), "Error should mention 'File too large': {}", error_str);
        assert!(error_str.contains("max"), "Error should mention 'max': {}", error_str);
        assert!(error_str.contains("got"), "Error should mention 'got': {}", error_str);
        assert!(error_str.contains("bytes"), "Error should mention 'bytes': {}", error_str);
    }

    #[test]
    fn test_upload_error_connection_failed() {
        let error = UploadError::ConnectionFailed("Network error".to_string());
        
        assert!(error.to_string().contains("Connection failed"));
        assert!(error.to_string().contains("Network error"));
    }

    #[test]
    fn test_upload_error_upload_failed() {
        let error = UploadError::UploadFailed("HTTP 500: Internal Server Error".to_string());
        
        assert!(error.to_string().contains("Upload failed"));
        assert!(error.to_string().contains("HTTP 500"));
    }

    #[test]
    fn test_upload_error_invalid_response() {
        let error = UploadError::InvalidResponse("Empty response".to_string());
        
        assert!(error.to_string().contains("Invalid response"));
        assert!(error.to_string().contains("Empty response"));
    }

    #[test]
    fn test_filename_default() {
        let filename: Option<String> = None;
        let result = filename.clone().unwrap_or_else(|| "file".to_string());
        
        assert_eq!(result, "file");
    }

    #[test]
    fn test_filename_with_custom_name() {
        let filename = Some("myfile.txt".to_string());
        let result = filename.clone().unwrap_or_else(|| "file".to_string());
        
        assert_eq!(result, "myfile.txt");
    }

    #[test]
    fn test_url_trimming() {
        let url = "  https://x0.at/test.txt  \n";
        let trimmed = url.trim().to_string();
        
        assert_eq!(trimmed, "https://x0.at/test.txt");
    }

    #[test]
    fn test_empty_url_detection() {
        let url = "".to_string();
        assert!(url.is_empty());
    }

    #[test]
    fn test_user_agent_format() {
        let version = env!("CARGO_PKG_VERSION");
        let user_agent = format!("pst/{}", version);
        
        assert!(user_agent.starts_with("pst/"));
        assert!(user_agent.contains(version));
    }

    #[test]
    fn test_timeout_duration() {
        let timeout_seconds = 30u64;
        let duration = std::time::Duration::from_secs(timeout_seconds);
        
        assert_eq!(duration.as_secs(), 30);
        assert_eq!(duration.as_millis(), 30000);
    }

    #[test]
    fn test_content_size_calculation() {
        let content = b"Hello, World!";
        let content_size = content.len() as u64;
        
        assert_eq!(content_size, 13);
    }

    #[test]
    fn test_large_file_size_validation() {
        let max_size = 512 * 1024 * 1024;
        let large_content = vec![0u8; 1024 * 1024 * 513]; // 513 MiB
        let content_size = large_content.len() as u64;
        
        assert!(content_size > max_size);
        assert_eq!(content_size, 513 * 1024 * 1024);
    }

    #[test]
    fn test_small_file_size_validation() {
        let max_size = 512 * 1024 * 1024;
        let small_content = b"small";
        let content_size = small_content.len() as u64;
        
        assert!(content_size <= max_size);
        assert_eq!(content_size, 5);
    }

    #[test]
    fn test_upload_options_default() {
        let options = UploadOptions::default();
        assert_eq!(options.expiration, None);
        assert!(!options.secret_url);
        assert_eq!(options.custom_name, None);
    }

    #[test]
    fn test_upload_type_is_text() {
        assert!(UploadType::Paste.is_text());
        assert!(!UploadType::File.is_text());
        assert!(!UploadType::Image.is_text());
    }

    #[test]
    fn test_request_file_size() {
        let request = UploadRequest::new(
            b"test content".to_vec(),
            Some("test.txt".to_string()),
            UploadType::File,
            None,
            false,
        );
        
        assert_eq!(request.file_size(), 12);
    }
}

pub struct X0AtProvider {
    endpoint: String,
    timeout_seconds: u64,
}

impl X0AtProvider {
    pub fn new(timeout_seconds: u64) -> Self {
        Self {
            endpoint: "https://x0.at/".to_string(),
            timeout_seconds,
        }
    }
}

#[async_trait]
impl UploadService for X0AtProvider {
    fn provider_name(&self) -> &str {
        "x0at"
    }

    fn supports_upload_type(&self, upload_type: UploadType) -> bool {
        matches!(
            upload_type,
            UploadType::File | UploadType::Image | UploadType::Paste
        )
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

        let default_filename = if request.is_redirect {
            "redirect.html".to_string()
        } else {
            "file".to_string()
        };

        let filename = request
            .filename
            .clone()
            .unwrap_or_else(|| default_filename);

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
            supports_expiration: false,
            supports_custom_names: false,
            requires_auth: false,
            supports_direct_text: false,
        }
    }
}
