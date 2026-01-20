use crate::models::{ProgressTracker, UploadRequest, UploadResponse, UploadType};
use crate::providers::{UploadError, UploadService};
use async_trait::async_trait;
use rand::Rng;

pub struct BunnyProvider {
    storage_zone: String,
    access_key: String,
    region: Option<String>,
    public_url: String,
    max_file_size_mb: u64,
    timeout_seconds: u64,
}

impl BunnyProvider {
    pub fn new(
        storage_zone: String,
        access_key: String,
        region: Option<String>,
        public_url: String,
        max_file_size_mb: u64,
        timeout_seconds: u64,
    ) -> Self {
        Self {
            storage_zone,
            access_key,
            region,
            public_url,
            max_file_size_mb,
            timeout_seconds,
        }
    }

    fn build_upload_url(&self, filename: &str) -> String {
        let host = match &self.region {
            Some(region) if !region.is_empty() => format!("{}.storage.bunnycdn.com", region),
            _ => "storage.bunnycdn.com".to_string(),
        };
        format!("https://{}/{}/{}", host, self.storage_zone, filename)
    }

    fn get_filename(&self, request: &UploadRequest) -> String {
        if let Some(name) = &request.filename {
            if name.starts_with("*.") {
                let ext = &name[1..];
                const CHARSET: &[u8] =
                    b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
                let mut rng = rand::thread_rng();
                let random: String = (0..8)
                    .map(|_| {
                        let idx = rng.gen::<usize>() % CHARSET.len();
                        CHARSET[idx] as char
                    })
                    .collect();
                return format!("{}{}", random, ext);
            }
            name.clone()
        } else if matches!(request.upload_type, UploadType::Paste) {
            const CHARSET: &[u8] =
                b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
            let mut rng = rand::thread_rng();
            let random: String = (0..8)
                .map(|_| {
                    let idx = rng.gen::<usize>() % CHARSET.len();
                    CHARSET[idx] as char
                })
                .collect();
            format!("{}.txt", random)
        } else {
            const CHARSET: &[u8] =
                b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
            let mut rng = rand::thread_rng();
            let random: String = (0..8)
                .map(|_| {
                    let idx = rng.gen::<usize>() % CHARSET.len();
                    CHARSET[idx] as char
                })
                .collect();
            format!("{}.bin", random)
        }
    }
}

#[async_trait]
impl UploadService for BunnyProvider {
    fn provider_name(&self) -> &str {
        "bunny"
    }

    fn supports_upload_type(&self, upload_type: UploadType) -> bool {
        matches!(
            upload_type,
            UploadType::File | UploadType::Image | UploadType::Paste
        )
    }

    fn max_file_size(&self) -> u64 {
        self.max_file_size_mb * 1024 * 1024
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

        let filename = self.get_filename(request);
        let upload_url = self.build_upload_url(&filename);

        let response = client
            .put(&upload_url)
            .header("AccessKey", &self.access_key)
            .header("Content-Type", "application/octet-stream")
            .body(request.content.clone())
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

        let final_url = format!("{}/{}", self.public_url, filename);

        Ok(UploadResponse::success(
            final_url,
            self.provider_name().to_string(),
            None,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_upload_url_with_region() {
        let provider = BunnyProvider::new(
            "my-storage-zone".to_string(),
            "test-key".to_string(),
            Some("ny".to_string()),
            "https://cdn.example.com".to_string(),
            500,
            30,
        );

        let url = provider.build_upload_url("test.png");
        assert_eq!(
            url,
            "https://ny.storage.bunnycdn.com/my-storage-zone/test.png"
        );
    }

    #[test]
    fn test_build_upload_url_without_region() {
        let provider = BunnyProvider::new(
            "my-storage-zone".to_string(),
            "test-key".to_string(),
            None,
            "https://cdn.example.com".to_string(),
            500,
            30,
        );

        let url = provider.build_upload_url("test.png");
        assert_eq!(url, "https://storage.bunnycdn.com/my-storage-zone/test.png");
    }

    #[test]
    fn test_build_upload_url_empty_region() {
        let provider = BunnyProvider::new(
            "my-storage-zone".to_string(),
            "test-key".to_string(),
            Some("".to_string()),
            "https://cdn.example.com".to_string(),
            500,
            30,
        );

        let url = provider.build_upload_url("test.png");
        assert_eq!(url, "https://storage.bunnycdn.com/my-storage-zone/test.png");
    }

    #[test]
    fn test_get_filename_with_extension() {
        let provider = BunnyProvider::new(
            "my-storage-zone".to_string(),
            "test-key".to_string(),
            None,
            "https://cdn.example.com".to_string(),
            500,
            30,
        );

        let request = UploadRequest::new(
            b"test content".to_vec(),
            Some("*.csv".to_string()),
            UploadType::File,
            None,
        );

        let filename = provider.get_filename(&request);
        assert!(filename.ends_with(".csv"));
        assert_eq!(filename.len(), 12); // 8 random chars + .csv
    }

    #[test]
    fn test_get_filename_with_custom_name() {
        let provider = BunnyProvider::new(
            "my-storage-zone".to_string(),
            "test-key".to_string(),
            None,
            "https://cdn.example.com".to_string(),
            500,
            30,
        );

        let request = UploadRequest::new(
            b"test content".to_vec(),
            Some("myfile.png".to_string()),
            UploadType::Image,
            None,
        );

        let filename = provider.get_filename(&request);
        assert_eq!(filename, "myfile.png");
    }

    #[test]
    fn test_get_filename_paste_generates_txt() {
        let provider = BunnyProvider::new(
            "my-storage-zone".to_string(),
            "test-key".to_string(),
            None,
            "https://cdn.example.com".to_string(),
            500,
            30,
        );

        let request = UploadRequest::new(b"test content".to_vec(), None, UploadType::Paste, None);

        let filename = provider.get_filename(&request);
        assert!(filename.ends_with(".txt"));
        assert_eq!(filename.len(), 12); // 8 random chars + .txt
    }

    #[test]
    fn test_get_filename_file_generates_bin() {
        let provider = BunnyProvider::new(
            "my-storage-zone".to_string(),
            "test-key".to_string(),
            None,
            "https://cdn.example.com".to_string(),
            500,
            30,
        );

        let request = UploadRequest::new(b"test content".to_vec(), None, UploadType::File, None);

        let filename = provider.get_filename(&request);
        assert!(filename.ends_with(".bin"));
        assert_eq!(filename.len(), 12); // 8 random chars + .bin
    }
}
