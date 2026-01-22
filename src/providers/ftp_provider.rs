use crate::models::{ProgressTracker, UploadRequest, UploadResponse, UploadType};
use crate::providers::{UploadError, UploadService};
use async_ssh2_lite::{AsyncSession, TokioTcpStream};
use async_trait::async_trait;
use futures_util::io::AsyncWriteExt;
use rand::Rng;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferProtocol {
    Ftp,
    Ftps,
    Sftp,
}

pub struct FTPProvider {
    protocol: TransferProtocol,
    host: String,
    port: u16,
    username: String,
    password: Option<String>,
    ssh_key_path: Option<PathBuf>,
    ssh_key_passphrase: Option<String>,
    directory: String,
    public_url: String,
    #[allow(dead_code)]
    directory_mode: DirectoryMode,
    max_file_size: u64,
    #[allow(dead_code)]
    ascii_mode_for_pastes: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryMode {
    ExistingOnly,
    CreateIfMissing,
}

impl TryFrom<&str> for DirectoryMode {
    type Error = String;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "existing_only" => Ok(DirectoryMode::ExistingOnly),
            "create_if_missing" => Ok(DirectoryMode::CreateIfMissing),
            _ => Err(format!("Unknown directory mode: {}", s)),
        }
    }
}

/// Configuration for creating an FTPProvider
pub struct FtpProviderConfig {
    pub protocol: TransferProtocol,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    pub ssh_key_path: Option<String>,
    pub ssh_key_passphrase: Option<String>,
    pub directory: String,
    pub public_url: String,
    pub directory_mode: DirectoryMode,
    pub max_file_size_mb: u64,
    pub ascii_mode_for_pastes: bool,
}

impl FTPProvider {
    pub fn new(config: FtpProviderConfig) -> Self {
        Self {
            protocol: config.protocol,
            host: config.host,
            port: config.port,
            username: config.username,
            password: config.password,
            ssh_key_path: config.ssh_key_path.map(PathBuf::from),
            ssh_key_passphrase: config.ssh_key_passphrase,
            directory: config.directory,
            public_url: config.public_url,
            directory_mode: config.directory_mode,
            max_file_size: config.max_file_size_mb * 1024 * 1024,
            ascii_mode_for_pastes: config.ascii_mode_for_pastes,
        }
    }

    fn get_filename(&self, request: &UploadRequest) -> String {
        if let Some(name) = &request.filename {
            // Check for * prefix which means "use this extension with random name"
            if name.starts_with("*.") {
                let ext = &name[1..]; // Remove * to get .ext
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
}

#[async_trait]
impl UploadService for FTPProvider {
    fn provider_name(&self) -> &str {
        "ftp_sftp"
    }

    fn supports_upload_type(&self, upload_type: UploadType) -> bool {
        matches!(
            upload_type,
            UploadType::File | UploadType::Image | UploadType::Paste
        )
    }

    fn max_file_size(&self) -> u64 {
        self.max_file_size
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

        let filename = self.get_filename(request);

        match self.protocol {
            TransferProtocol::Sftp => self.upload_sftp(request, &filename, progress).await,
            TransferProtocol::Ftps => self.upload_ftps(request, &filename, progress).await,
            TransferProtocol::Ftp => self.upload_ftp(request, &filename, progress).await,
        }
    }
}

impl FTPProvider {
    async fn upload_sftp(
        &self,
        request: &UploadRequest,
        filename: &str,
        progress: Option<&ProgressTracker>,
    ) -> Result<UploadResponse, UploadError> {
        let stream = TokioTcpStream::connect(format!("{}:{}", self.host, self.port))
            .await
            .map_err(|e| UploadError::ConnectionFailed(e.to_string()))?;

        let mut session = AsyncSession::new(stream, None)
            .map_err(|e| UploadError::ConnectionFailed(e.to_string()))?;

        session
            .handshake()
            .await
            .map_err(|e| UploadError::ConnectionFailed(e.to_string()))?;

        let auth_result = if let Some(ref key_path) = self.ssh_key_path {
            if tokio::fs::metadata(key_path).await.is_ok() {
                let key_path = std::path::Path::new(key_path);
                session
                    .userauth_pubkey_file(
                        &self.username,
                        None,
                        key_path,
                        self.ssh_key_passphrase.as_deref(),
                    )
                    .await
            } else if let Some(ref password) = self.password {
                session.userauth_password(&self.username, password).await
            } else {
                return Err(UploadError::AuthenticationFailed);
            }
        } else if let Some(ref password) = self.password {
            session.userauth_password(&self.username, password).await
        } else {
            return Err(UploadError::AuthenticationFailed);
        };

        auth_result.map_err(|_| UploadError::AuthenticationFailed)?;

        if !session.authenticated() {
            return Err(UploadError::AuthenticationFailed);
        }

        let sftp = session
            .sftp()
            .await
            .map_err(|e| UploadError::ConnectionFailed(e.to_string()))?;

        let remote_path = Path::new(&self.directory).join(filename);

        let mut remote_file = sftp
            .create(&remote_path)
            .await
            .map_err(|e| UploadError::UploadFailed(format!("Failed to create file: {}", e)))?;

        let chunk_size = 32 * 1024;
        let mut offset = 0;
        while offset < request.content.len() {
            let len = std::cmp::min(chunk_size, request.content.len() - offset);
            remote_file
                .write_all(&request.content[offset..offset + len])
                .await
                .map_err(|e| UploadError::UploadFailed(format!("Failed to write file: {}", e)))?;
            if let Some(p) = progress {
                p.add_progress(len as u64);
            }
            offset += len;
        }

        let url = format!("{}/{}", self.public_url, filename);

        Ok(UploadResponse::success(
            url,
            format!("sftp ({}@{})", self.username, self.host),
            None,
        ))
    }

    async fn upload_ftp(
        &self,
        _request: &UploadRequest,
        _filename: &str,
        _progress: Option<&ProgressTracker>,
    ) -> Result<UploadResponse, UploadError> {
        Err(UploadError::ProviderNotAvailable(
            "Plain FTP is not supported, use FTPS or SFTP instead".to_string(),
        ))
    }

    async fn upload_ftps(
        &self,
        _request: &UploadRequest,
        _filename: &str,
        _progress: Option<&ProgressTracker>,
    ) -> Result<UploadResponse, UploadError> {
        Err(UploadError::ProviderNotAvailable(
            "FTPS support coming soon, use SFTP for now".to_string(),
        ))
    }
}
