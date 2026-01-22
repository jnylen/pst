use crate::config::{Config, ProviderConfig};
use crate::models::{
    ProgressTracker, UploadRequest, UploadResponse, UploadType, VerboseProgressCallback,
};
use crate::providers::{
    BunnyProvider, DirectoryMode, FTPProvider, FtpProviderConfig, PasteRsProvider,
    TransferProtocol, UguuProvider, UploadError, UploadService, X0AtProvider, ZeroX0STProvider,
};
use std::collections::HashMap;
use std::sync::Arc;

pub struct UploadOrchestrator {
    providers: Vec<Box<dyn UploadService>>,
    provider_names: HashMap<String, usize>,
    config: Arc<Config>,
    max_retries: u32,
    retry_delay_ms: u64,
    #[allow(dead_code)]
    timeout_seconds: u64,
}

impl UploadOrchestrator {
    pub fn new(config: Arc<Config>) -> Self {
        let mut providers: Vec<Box<dyn UploadService>> = Vec::new();
        let mut provider_names: HashMap<String, usize> = HashMap::new();

        let timeout_seconds = config.general.timeout_seconds;
        let max_retries = config.general.max_retries;
        let retry_delay_ms = config.general.retry_delay_ms;

        for (name, provider_config) in config.providers.iter() {
            if let Some(provider) = create_provider(name.as_str(), provider_config, timeout_seconds)
            {
                let index = providers.len();
                providers.push(provider);
                provider_names.insert(name.clone(), index);
            }
        }

        Self {
            providers,
            provider_names,
            config,
            max_retries,
            retry_delay_ms,
            timeout_seconds,
        }
    }

    pub fn create_progress_tracker(
        &self,
        request: &UploadRequest,
        provider_name: &str,
        show_progress: bool,
    ) -> Option<ProgressTracker> {
        if !show_progress {
            return None;
        }

        let callback = Arc::new(VerboseProgressCallback::new(true));
        Some(ProgressTracker::new(
            request.content.len() as u64,
            callback,
            provider_name.to_string(),
        ))
    }

    pub async fn upload(
        &self,
        request: &UploadRequest,
        group: &str,
        progress: Option<&ProgressTracker>,
    ) -> UploadResponse {
        let provider_indices = self.get_provider_indices_for_group(group, &request.upload_type);

        if provider_indices.is_empty() {
            return UploadResponse::failed(
                "orchestrator".to_string(),
                format!("No providers available for group: {}", group),
            );
        }

        if let Some(p) = progress {
            p.add_progress(0);
        }

        let mut errors: Vec<UploadResponse> = Vec::new();

        for &index in &provider_indices {
            let provider = self.providers[index].as_ref();
            match self.try_upload(provider, request, progress).await {
                Ok(response) if response.success => {
                    if let Some(p) = progress {
                        p.finish();
                    }
                    return response;
                }
                Ok(response) => {
                    errors.push(response);
                }
                Err(error) => {
                    errors.push(UploadResponse::failed(
                        provider.provider_name().to_string(),
                        error.to_string(),
                    ));
                }
            }
        }

        UploadResponse::all_providers_failed(errors)
    }

    pub async fn upload_to_specific_provider(
        &self,
        request: &UploadRequest,
        provider_name: &str,
        progress: Option<&ProgressTracker>,
    ) -> UploadResponse {
        let provider_index = self
            .providers
            .iter()
            .enumerate()
            .find(|(_, p)| p.provider_name() == provider_name)
            .map(|(index, _)| index);

        if let Some(index) = provider_index {
            let provider = self.providers[index].as_ref();

            if !provider.supports_upload_type(request.upload_type.clone()) {
                return UploadResponse::failed(
                    provider_name.to_string(),
                    format!(
                        "Provider '{}' does not support this upload type",
                        provider_name
                    ),
                );
            }

            if let Some(p) = progress {
                p.add_progress(0);
            }

            match self.try_upload(provider, request, progress).await {
                Ok(response) if response.success => {
                    if let Some(p) = progress {
                        p.finish();
                    }
                    response
                }
                Ok(response) => response,
                Err(error) => UploadResponse::failed(provider_name.to_string(), error.to_string()),
            }
        } else {
            UploadResponse::failed(
                provider_name.to_string(),
                format!(
                    "Unknown provider: {}. Available providers: 0x0st, paste_rs, uguu, x0at, ftp_sftp, bunny",
                    provider_name
                ),
            )
        }
    }

    fn get_provider_indices_for_group(&self, group: &str, upload_type: &UploadType) -> Vec<usize> {
        let provider_names = self.config.get_providers_for_group(group);

        provider_names
            .into_iter()
            .filter_map(|(name, _)| self.provider_names.get(&name).copied())
            .filter(|&index| self.providers[index].supports_upload_type(upload_type.clone()))
            .collect()
    }

    async fn try_upload(
        &self,
        provider: &dyn UploadService,
        request: &UploadRequest,
        progress: Option<&ProgressTracker>,
    ) -> Result<UploadResponse, UploadError> {
        let content_size = request.content.len() as u64;
        if content_size > provider.max_file_size() {
            return Err(UploadError::FileTooLarge {
                max_size: provider.max_file_size(),
                actual_size: content_size,
            });
        }

        if !provider.test_connection().await {
            return Err(UploadError::ConnectionFailed(format!(
                "Cannot connect to {}",
                provider.provider_name()
            )));
        }

        let mut retries = 0;
        let mut last_error = None;

        while retries <= self.max_retries {
            match provider.upload(request, progress).await {
                Ok(response) => return Ok(response),
                Err(error) => {
                    last_error = Some(error);

                    if retries < self.max_retries {
                        let delay = self.retry_delay_ms * (2_u64.pow(retries));
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        retries += 1;
                    } else {
                        break;
                    }
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| UploadError::UploadFailed("Max retries exceeded".to_string())))
    }
}

fn create_provider(
    name: &str,
    config: &ProviderConfig,
    timeout_seconds: u64,
) -> Option<Box<dyn UploadService>> {
    match name.to_lowercase().as_str() {
        "0x0st" | "0x0.st" => Some(Box::new(ZeroX0STProvider::new(timeout_seconds))),
        "paste_rs" | "paste.rs" => Some(Box::new(PasteRsProvider::new(timeout_seconds))),
        "uguu" | "uguu.se" => Some(Box::new(UguuProvider::new(timeout_seconds))),
        "x0at" | "x0.at" => Some(Box::new(X0AtProvider::new(timeout_seconds))),
        "ftp_sftp" | "ftp" | "sftp" => {
            if let ProviderConfig::FtpSftp(ftp_config) = config {
                // Determine which protocol to use
                let protocol = if ftp_config.enable_sftp {
                    TransferProtocol::Sftp
                } else if ftp_config.enable_ftps {
                    TransferProtocol::Ftps
                } else {
                    TransferProtocol::Ftp
                };

                let ssh_key_path = ftp_config
                    .ssh_private_key
                    .clone()
                    .map(|s| shellexpand::tilde(&s).into_owned());

                let directory_mode = DirectoryMode::try_from(ftp_config.directory_mode.as_str())
                    .unwrap_or(DirectoryMode::CreateIfMissing);

                Some(Box::new(FTPProvider::new(FtpProviderConfig {
                    protocol,
                    host: ftp_config.host.clone(),
                    port: ftp_config.port,
                    username: ftp_config.username.clone(),
                    password: ftp_config.password.clone(),
                    ssh_key_path,
                    ssh_key_passphrase: ftp_config.ssh_key_passphrase.clone(),
                    directory: ftp_config.directory.clone(),
                    public_url: ftp_config.public_url.clone(),
                    directory_mode,
                    max_file_size_mb: ftp_config.max_file_size_mb,
                    ascii_mode_for_pastes: ftp_config.ascii_mode_for_pastes,
                })))
            } else {
                None
            }
        }
        "bunny" | "bunnycdn" => {
            if let ProviderConfig::Bunny(bunny_config) = config {
                Some(Box::new(BunnyProvider::new(
                    bunny_config.storage_zone.clone(),
                    bunny_config.access_key.clone(),
                    bunny_config.region.clone(),
                    bunny_config.public_url.clone(),
                    bunny_config.max_file_size_mb,
                    timeout_seconds,
                )))
            } else {
                None
            }
        }
        _ => None,
    }
}
