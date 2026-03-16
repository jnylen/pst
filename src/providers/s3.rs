use crate::models::{ProgressTracker, UploadRequest, UploadResponse, UploadType};
use crate::providers::{UploadError, UploadService};
use async_trait::async_trait;
use aws_config::meta::region::RegionProviderChain;
use aws_credential_types::Credentials;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use mime_guess;
use rand::Rng;

pub struct S3Provider {
    bucket: String,
    region: String,
    endpoint: Option<String>,
    access_key_id: String,
    secret_access_key: String,
    public_url: String,
    max_file_size_mb: u64,
    multipart_threshold_mb: u64,
    multipart_chunk_size_mb: u64,
    timeout_seconds: u64,
}

impl S3Provider {
    pub fn new(
        bucket: String,
        region: String,
        endpoint: Option<String>,
        access_key_id: String,
        secret_access_key: String,
        public_url: String,
        max_file_size_mb: u64,
        multipart_threshold_mb: u64,
        multipart_chunk_size_mb: u64,
        timeout_seconds: u64,
    ) -> Self {
        Self {
            bucket,
            region,
            endpoint,
            access_key_id,
            secret_access_key,
            public_url,
            max_file_size_mb,
            multipart_threshold_mb,
            multipart_chunk_size_mb,
            timeout_seconds,
        }
    }

    async fn create_client(&self) -> Result<Client, UploadError> {
        let region_provider = RegionProviderChain::first_try(aws_sdk_s3::config::Region::new(self.region.clone()));
        
        let credentials = Credentials::new(
            &self.access_key_id,
            &self.secret_access_key,
            None,
            None,
            "pst-config",
        );

        let mut config_builder = aws_sdk_s3::config::Builder::new()
            .region(region_provider.region().await)
            .credentials_provider(credentials)
            .timeout_config(
                aws_config::timeout::TimeoutConfig::builder()
                    .operation_timeout(std::time::Duration::from_secs(self.timeout_seconds))
                    .build(),
            );

        // Set custom endpoint for S3-compatible services (MinIO, DigitalOcean Spaces, etc.)
        if let Some(endpoint) = &self.endpoint {
            config_builder = config_builder.endpoint_url(endpoint);
            // For S3-compatible services, we need to force path style
            config_builder = config_builder.force_path_style(true);
        }

        let config = config_builder.build();
        Ok(Client::from_conf(config))
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
        } else if request.is_redirect {
            const CHARSET: &[u8] =
                b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
            let mut rng = rand::thread_rng();
            let random: String = (0..8)
                .map(|_| {
                    let idx = rng.gen::<usize>() % CHARSET.len();
                    CHARSET[idx] as char
                })
                .collect();
            format!("{}.html", random)
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

    async fn upload_single(
        &self,
        client: &Client,
        key: &str,
        content: &[u8],
        content_type: &str,
        progress: Option<&ProgressTracker>,
    ) -> Result<(), UploadError> {
        let byte_stream = ByteStream::from(content.to_vec());
        
        client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(byte_stream)
            .content_type(content_type)
            .acl(aws_sdk_s3::types::ObjectCannedAcl::PublicRead)
            .send()
            .await
            .map_err(|e| UploadError::UploadFailed(format!("S3 put_object failed: {}", e)))?;

        if let Some(p) = progress {
            p.add_progress(content.len() as u64);
        }

        Ok(())
    }

    async fn upload_multipart(
        &self,
        client: &Client,
        key: &str,
        content: &[u8],
        content_type: &str,
        progress: Option<&ProgressTracker>,
    ) -> Result<(), UploadError> {
        let chunk_size = (self.multipart_chunk_size_mb * 1024 * 1024) as usize;
        let total_size = content.len();

        // Initiate multipart upload
        let create_response = client
            .create_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .acl(aws_sdk_s3::types::ObjectCannedAcl::PublicRead)
            .send()
            .await
            .map_err(|e| UploadError::UploadFailed(format!("Failed to create multipart upload: {}", e)))?;

        let upload_id = create_response.upload_id
            .ok_or_else(|| UploadError::UploadFailed("No upload ID returned".to_string()))?;

        // Upload parts
        let mut completed_parts = Vec::new();
        let part_count = (total_size + chunk_size - 1) / chunk_size;

        for part_number in 1..=part_count {
            let start = (part_number - 1) * chunk_size;
            let end = std::cmp::min(start + chunk_size, total_size);
            let part_data = &content[start..end];

            let byte_stream = ByteStream::from(part_data.to_vec());
            
            let upload_part_response = client
                .upload_part()
                .bucket(&self.bucket)
                .key(key)
                .upload_id(&upload_id)
                .part_number(part_number as i32)
                .body(byte_stream)
                .send()
                .await
                .map_err(|e| {
                    // Try to abort multipart upload on failure
                    let _ = tokio::runtime::Handle::current().block_on(
                        client.abort_multipart_upload()
                            .bucket(&self.bucket)
                            .key(key)
                            .upload_id(&upload_id)
                            .send()
                    );
                    UploadError::UploadFailed(format!("Failed to upload part {}: {}", part_number, e))
                })?;

            completed_parts.push(
                CompletedPart::builder()
                    .e_tag(upload_part_response.e_tag.unwrap_or_default())
                    .part_number(part_number as i32)
                    .build(),
            );

            if let Some(p) = progress {
                p.add_progress(part_data.len() as u64);
            }
        }

        // Complete multipart upload
        let completed_multipart = CompletedMultipartUpload::builder()
            .set_parts(Some(completed_parts))
            .build();

        client
            .complete_multipart_upload()
            .bucket(&self.bucket)
            .key(key)
            .upload_id(upload_id)
            .multipart_upload(completed_multipart)
            .send()
            .await
            .map_err(|e| UploadError::UploadFailed(format!("Failed to complete multipart upload: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl UploadService for S3Provider {
    fn provider_name(&self) -> &str {
        "s3"
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
        progress: Option<&ProgressTracker>,
    ) -> Result<UploadResponse, UploadError> {
        let content_size = request.content.len() as u64;

        if content_size > self.max_file_size() {
            return Err(UploadError::FileTooLarge {
                max_size: self.max_file_size(),
                actual_size: content_size,
            });
        }

        let client = self.create_client().await?;
        let key = self.get_filename(request);

        // Determine MIME type from filename or upload type
        let content_type = if request.is_redirect {
            "text/html".to_string()
        } else if let Some(filename) = &request.filename {
            mime_guess::from_path(filename)
                .first()
                .map(|mime| mime.to_string())
                .unwrap_or_else(|| "application/octet-stream".to_string())
        } else {
            match request.upload_type {
                UploadType::Paste => "text/plain".to_string(),
                _ => "application/octet-stream".to_string(),
            }
        };

        // Determine if we need multipart upload
        let multipart_threshold = self.multipart_threshold_mb * 1024 * 1024;
        
        if content_size > multipart_threshold {
            self.upload_multipart(&client, &key, &request.content, &content_type, progress).await?;
        } else {
            self.upload_single(&client, &key, &request.content, &content_type, progress).await?;
        }

        let final_url = format!("{}/{}", self.public_url.trim_end_matches('/'), key);

        Ok(UploadResponse::success(
            final_url,
            self.provider_name().to_string(),
            None,
        ))
    }

    async fn test_connection(&self) -> bool {
        match self.create_client().await {
            Ok(client) => {
                // Try to list objects (with max 1) to test connection
                match client.list_objects_v2()
                    .bucket(&self.bucket)
                    .max_keys(1)
                    .send()
                    .await {
                    Ok(_) => true,
                    Err(_) => false,
                }
            }
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_filename_with_extension() {
        let provider = S3Provider::new(
            "my-bucket".to_string(),
            "us-east-1".to_string(),
            None,
            "AKIAIOSFODNN7EXAMPLE".to_string(),
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            "https://my-bucket.s3.amazonaws.com".to_string(),
            5000,
            100,
            10,
            30,
        );

        let request = UploadRequest::new(
            b"test content".to_vec(),
            Some("*.csv".to_string()),
            UploadType::File,
            None,
            false,
        );

        let filename = provider.get_filename(&request);
        assert!(filename.ends_with(".csv"));
        assert_eq!(filename.len(), 12); // 8 random chars + .csv
    }

    #[test]
    fn test_get_filename_with_custom_name() {
        let provider = S3Provider::new(
            "my-bucket".to_string(),
            "us-east-1".to_string(),
            None,
            "AKIAIOSFODNN7EXAMPLE".to_string(),
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            "https://my-bucket.s3.amazonaws.com".to_string(),
            5000,
            100,
            10,
            30,
        );

        let request = UploadRequest::new(
            b"test content".to_vec(),
            Some("myfile.png".to_string()),
            UploadType::Image,
            None,
            false,
        );

        let filename = provider.get_filename(&request);
        assert_eq!(filename, "myfile.png");
    }

    #[test]
    fn test_get_filename_paste_generates_txt() {
        let provider = S3Provider::new(
            "my-bucket".to_string(),
            "us-east-1".to_string(),
            None,
            "AKIAIOSFODNN7EXAMPLE".to_string(),
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            "https://my-bucket.s3.amazonaws.com".to_string(),
            5000,
            100,
            10,
            30,
        );

        let request = UploadRequest::new(b"test content".to_vec(), None, UploadType::Paste, None, false);

        let filename = provider.get_filename(&request);
        assert!(filename.ends_with(".txt"));
        assert_eq!(filename.len(), 12); // 8 random chars + .txt
    }

    #[test]
    fn test_get_filename_redirect_generates_html() {
        let provider = S3Provider::new(
            "my-bucket".to_string(),
            "us-east-1".to_string(),
            None,
            "AKIAIOSFODNN7EXAMPLE".to_string(),
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            "https://my-bucket.s3.amazonaws.com".to_string(),
            5000,
            100,
            10,
            30,
        );

        let request = UploadRequest::new(b"html content".to_vec(), None, UploadType::Paste, None, true);

        let filename = provider.get_filename(&request);
        assert!(filename.ends_with(".html"));
        assert_eq!(filename.len(), 13); // 8 random chars + .html
    }

    #[test]
    fn test_get_filename_file_generates_bin() {
        let provider = S3Provider::new(
            "my-bucket".to_string(),
            "us-east-1".to_string(),
            None,
            "AKIAIOSFODNN7EXAMPLE".to_string(),
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            "https://my-bucket.s3.amazonaws.com".to_string(),
            5000,
            100,
            10,
            30,
        );

        let request = UploadRequest::new(b"test content".to_vec(), None, UploadType::File, None, false);

        let filename = provider.get_filename(&request);
        assert!(filename.ends_with(".bin"));
        assert_eq!(filename.len(), 12); // 8 random chars + .bin
    }
}