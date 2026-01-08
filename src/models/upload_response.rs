use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct UploadResponse {
    pub success: bool,
    pub url: Option<String>,
    pub provider: String,
    pub error: Option<String>,
    #[allow(dead_code)]
    pub metadata: Option<ResponseMetadata>,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct ResponseMetadata {
    pub filename: Option<String>,
    pub file_size: Option<u64>,
    pub expiration: Option<String>,
    pub provider_specific: HashMap<String, String>,
}

impl UploadResponse {
    pub fn success(url: String, provider: String, metadata: Option<ResponseMetadata>) -> Self {
        Self {
            success: true,
            url: Some(url),
            provider,
            error: None,
            metadata,
        }
    }

    pub fn failed(provider: String, error: String) -> Self {
        Self {
            success: false,
            url: None,
            provider,
            error: Some(error),
            metadata: None,
        }
    }

    pub fn all_providers_failed(errors: Vec<UploadResponse>) -> Self {
        let errors_str: Vec<String> = errors
            .iter()
            .filter(|e| !e.success)
            .map(|e| {
                format!(
                    "{}: {}",
                    e.provider,
                    e.error.clone().unwrap_or_else(|| "Unknown".to_string())
                )
            })
            .collect();

        Self {
            success: false,
            url: None,
            provider: "all".to_string(),
            error: Some(format!("All providers failed: {}", errors_str.join("; "))),
            metadata: None,
        }
    }
}
