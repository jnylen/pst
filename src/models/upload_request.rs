#[derive(Debug, Clone)]
pub struct UploadRequest {
    pub content: Vec<u8>,
    pub filename: Option<String>,
    pub upload_type: UploadType,
    #[allow(dead_code)]
    pub options: UploadOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadType {
    Paste,
    File,
    Image,
}

#[derive(Debug, Clone, Default)]
pub struct UploadOptions {
    #[allow(dead_code)]
    pub expiration: Option<String>,
    #[allow(dead_code)]
    pub secret_url: bool,
    #[allow(dead_code)]
    pub custom_name: Option<String>,
}

impl UploadRequest {
    pub fn new(
        content: Vec<u8>,
        filename: Option<String>,
        upload_type: UploadType,
        options: Option<UploadOptions>,
    ) -> Self {
        Self {
            content,
            filename,
            upload_type,
            options: options.unwrap_or_default(),
        }
    }

    #[allow(dead_code)]
    pub fn file_size(&self) -> u64 {
        self.content.len() as u64
    }
}

impl UploadType {
    #[allow(dead_code)]
    pub fn is_text(&self) -> bool {
        matches!(self, UploadType::Paste)
    }
}
