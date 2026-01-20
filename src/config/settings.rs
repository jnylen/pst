use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Config file not found: {0}")]
    #[allow(dead_code)]
    NotFound(PathBuf),

    #[error("Failed to parse config: {0}")]
    #[allow(dead_code)]
    ParseError(String),

    #[error("Invalid config value: {0}")]
    InvalidValue(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    TomlParseError(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSerializeError(#[from] toml::ser::Error),
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Config {
    pub general: GeneralConfig,
    pub providers: HashMap<String, ProviderConfig>,
    #[serde(default)]
    pub provider_groups: HashMap<String, ProviderGroupConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct GeneralConfig {
    pub default_provider: String,
    pub timeout_seconds: u64,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
    #[serde(default = "default_copy_to_clipboard")]
    pub copy_to_clipboard: bool,
    #[serde(default = "default_strip_exif")]
    pub strip_exif: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum ProviderConfig {
    #[serde(rename = "http")]
    Http(HttpProviderConfig),
    #[serde(rename = "ftp_sftp")]
    FtpSftp(FTPSFTPProviderConfig),
    #[serde(rename = "bunny")]
    Bunny(BunnyProviderConfig),
}

impl ProviderConfig {
    pub fn is_enabled(&self) -> bool {
        match self {
            ProviderConfig::Http(config) => config.enabled,
            ProviderConfig::FtpSftp(config) => config.enabled,
            ProviderConfig::Bunny(config) => config.enabled,
        }
    }

    #[allow(dead_code)]
    pub fn get_max_file_size(&self) -> u64 {
        match self {
            ProviderConfig::Http(config) => config.max_file_size_mb,
            ProviderConfig::FtpSftp(config) => config.max_file_size_mb,
            ProviderConfig::Bunny(config) => config.max_file_size_mb,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct HttpProviderConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_max_file_size")]
    pub max_file_size_mb: u64,
    #[serde(default = "default_ascii_mode")]
    pub ascii_mode_for_pastes: bool,
    #[serde(default)]
    pub userhash: Option<String>,
    #[serde(default = "default_expiration")]
    pub default_expiration: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FTPSFTPProviderConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    pub ssh_private_key: Option<String>,
    pub ssh_key_passphrase: Option<String>,
    pub directory: String,
    pub public_url: String,
    #[serde(default = "default_directory_mode")]
    pub directory_mode: String,
    #[serde(default = "default_max_file_size")]
    pub max_file_size_mb: u64,
    #[serde(default = "default_ascii_mode")]
    pub ascii_mode_for_pastes: bool,
    #[serde(default)]
    pub enable_ftp: bool,
    #[serde(default)]
    pub enable_ftps: bool,
    #[serde(default)]
    pub enable_sftp: bool,
    #[serde(default = "default_expiration")]
    pub default_expiration: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct BunnyProviderConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub storage_zone: String,
    pub access_key: String,
    #[serde(default)]
    pub region: Option<String>,
    pub public_url: String,
    #[serde(default = "default_max_file_size")]
    pub max_file_size_mb: u64,
}

fn default_enabled() -> bool {
    true
}

fn default_port() -> u16 {
    21
}

fn default_directory_mode() -> String {
    "create_if_missing".to_string()
}

fn default_max_file_size() -> u64 {
    1000
}

fn default_ascii_mode() -> bool {
    true
}

fn default_expiration() -> String {
    "1h".to_string()
}

fn default_copy_to_clipboard() -> bool {
    false
}

fn default_strip_exif() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ProviderGroupConfig {
    pub providers: Vec<String>,
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        let config_path = get_config_path()?;

        if !config_path.exists() {
            let default_config = Config::default_with_ftp();

            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let config_content =
                toml::to_string_pretty(&default_config).map_err(ConfigError::TomlSerializeError)?;
            std::fs::write(&config_path, config_content)?;

            return Ok(default_config);
        }

        let config_content = std::fs::read_to_string(&config_path)?;
        let config: Config =
            toml::from_str(&config_content).map_err(ConfigError::TomlParseError)?;

        Ok(config)
    }

    pub fn default_with_ftp() -> Self {
        Self {
            general: GeneralConfig {
                default_provider: "all".to_string(),
                timeout_seconds: 30,
                max_retries: 3,
                retry_delay_ms: 1000,
                copy_to_clipboard: false,
                strip_exif: true,
            },
            providers: {
                let mut map = HashMap::new();

                // FTP/SFTP first
                map.insert(
                    "ftp_sftp".to_string(),
                    ProviderConfig::FtpSftp(FTPSFTPProviderConfig {
                        enabled: false,
                        host: "ftp.example.com".to_string(),
                        port: 22,
                        username: "username".to_string(),
                        password: Some("password".to_string()),
                        ssh_private_key: Some("~/.ssh/id_rsa".to_string()),
                        ssh_key_passphrase: None,
                        directory: "/public_html/uploads".to_string(),
                        public_url: "https://cdn.example.com/uploads".to_string(),
                        directory_mode: "create_if_missing".to_string(),
                        max_file_size_mb: 1000,
                        ascii_mode_for_pastes: true,
                        enable_ftp: false,
                        enable_ftps: false,
                        enable_sftp: true,
                        default_expiration: "1h".to_string(),
                    }),
                );

                // HTTP providers
                map.insert(
                    "0x0st".to_string(),
                    ProviderConfig::Http(HttpProviderConfig {
                        enabled: true,
                        max_file_size_mb: 512,
                        ascii_mode_for_pastes: true,
                        userhash: None,
                        default_expiration: "1h".to_string(),
                    }),
                );

                map.insert(
                    "paste_rs".to_string(),
                    ProviderConfig::Http(HttpProviderConfig {
                        enabled: true,
                        max_file_size_mb: 10,
                        ascii_mode_for_pastes: true,
                        userhash: None,
                        default_expiration: "1h".to_string(),
                    }),
                );

                map.insert(
                    "uguu".to_string(),
                    ProviderConfig::Http(HttpProviderConfig {
                        enabled: true,
                        max_file_size_mb: 128,
                        ascii_mode_for_pastes: true,
                        userhash: None,
                        default_expiration: "1h".to_string(),
                    }),
                );

                // Bunny CDN - requires explicit configuration
                map.insert(
                    "bunny".to_string(),
                    ProviderConfig::Bunny(BunnyProviderConfig {
                        enabled: false,
                        storage_zone: "your-storage-zone".to_string(),
                        access_key: "your-access-key".to_string(),
                        region: None,
                        public_url: "https://cdn.example.com/files".to_string(),
                        max_file_size_mb: 500,
                    }),
                );

                map
            },
            provider_groups: {
                let mut map = HashMap::new();
                // FTP/SFTP at the top
                map.insert(
                    "files".to_string(),
                    ProviderGroupConfig {
                        providers: vec![
                            "ftp_sftp".to_string(),
                            "bunny".to_string(),
                            "0x0st".to_string(),
                            "uguu".to_string(),
                        ],
                    },
                );
                map.insert(
                    "pastes".to_string(),
                    ProviderGroupConfig {
                        providers: vec![
                            "ftp_sftp".to_string(),
                            "bunny".to_string(),
                            "paste_rs".to_string(),
                        ],
                    },
                );
                map.insert(
                    "images".to_string(),
                    ProviderGroupConfig {
                        providers: vec![
                            "ftp_sftp".to_string(),
                            "bunny".to_string(),
                            "0x0st".to_string(),
                            "uguu".to_string(),
                        ],
                    },
                );
                map
            },
        }
    }

    #[allow(dead_code)]
    pub fn get_provider_config(&self, name: &str) -> Option<&ProviderConfig> {
        self.providers.get(name)
    }

    pub fn get_provider_group(&self, name: &str) -> Option<&Vec<String>> {
        self.provider_groups.get(name).map(|g| &g.providers)
    }

    pub fn get_providers_for_group(&self, group: &str) -> Vec<(String, &ProviderConfig)> {
        if let Some(provider_names) = self.get_provider_group(group) {
            provider_names
                .iter()
                .filter_map(|name| {
                    self.providers
                        .get(name)
                        .filter(|config| config.is_enabled())
                        .map(|config| (name.clone(), config))
                })
                .collect()
        } else {
            self.providers
                .iter()
                .filter(|(_, config)| config.is_enabled())
                .map(|(name, config)| (name.clone(), config))
                .collect()
        }
    }
}

fn get_config_path() -> Result<PathBuf, ConfigError> {
    let project_dirs = ProjectDirs::from("", "", "pst").ok_or_else(|| {
        ConfigError::InvalidValue("Could not determine home directory".to_string())
    })?;

    let config_dir = project_dirs.config_dir().to_path_buf();

    Ok(config_dir.join("config.toml"))
}
