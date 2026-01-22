use crate::clipboard::ClipboardContent;
use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{stdin, AsyncReadExt};

mod clipboard;
mod config;
mod exif;
mod models;
mod orchestrator;
mod providers;
mod redirect_generator;

fn copy_to_clipboard(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    use arboard::Clipboard;
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(text)?;
    Ok(())
}

#[derive(Parser, Debug)]
#[clap(name = env!("CARGO_PKG_NAME"))]
#[clap(author = env!("CARGO_PKG_AUTHORS"))]
#[clap(version = env!("CARGO_PKG_VERSION"))]
#[clap(about = env!("CARGO_PKG_DESCRIPTION"))]
struct Args {
    /// File to upload
    #[clap(
        short,
        long,
        value_name = "FILE",
        conflicts_with = "clipboard",
        conflicts_with = "input_file"
    )]
    file: Option<String>,

    /// File to upload (positional argument)
    #[clap(value_name = "FILE", index = 1, conflicts_with = "clipboard")]
    input_file: Option<String>,

    /// Upload from clipboard
    #[clap(short = 'c', long, conflicts_with = "file")]
    clipboard: bool,

    /// Custom filename for the upload
    #[clap(short = 'n', long, value_name = "FILENAME")]
    filename: Option<String>,

    /// Output format
    #[clap(short, long, value_name = "FORMAT", default_value = "url")]
    output: OutputFormat,

    /// Provider group to use (files, pastes, images)
    #[clap(short, long, value_name = "GROUP")]
    group: Option<String>,

    /// Force specific provider
    #[clap(short, long, value_name = "PROVIDER")]
    provider: Option<String>,

    /// Set expiration time
    #[clap(short, long, value_name = "EXPIRES")]
    expires: Option<String>,

    /// Show progress bar
    #[clap(long)]
    progress: bool,

    /// Copy URL to clipboard after upload
    #[clap(long)]
    copy_to_clipboard: bool,

    /// Keep EXIF metadata when uploading images (disabled by default)
    #[clap(long)]
    no_exif: bool,

    /// Create a redirect HTML file that redirects to the provided URL
    #[clap(short, long, value_name = "URL", conflicts_with = "file", conflicts_with = "input_file", conflicts_with = "clipboard")]
    redirect: Option<String>,
}

fn get_file_path(args: &Args) -> Result<Option<&String>> {
    match (&args.file, &args.input_file) {
        (Some(_), Some(_)) => {
            anyhow::bail!("Cannot specify both -f/--file and a positional file argument")
        }
        (Some(f), None) => Ok(Some(f)),
        (None, Some(f)) => Ok(Some(f)),
        (None, None) => Ok(None),
    }
}

#[derive(Debug, Clone, ValueEnum)]
enum OutputFormat {
    Url,
    Json,
    Verbose,
}

fn is_stdin_pipe() -> bool {
    !atty::is(atty::Stream::Stdin)
}

fn is_binary_content(content: &[u8]) -> bool {
    if content.is_empty() {
        return false;
    }

    let sample_size = std::cmp::min(content.len(), 8192);
    let sample = &content[..sample_size];

    let null_bytes = sample.iter().filter(|&&b| b == 0).count();
    let total_bytes = sample.len();

    (null_bytes * 100) > total_bytes
}

fn is_definitely_text(content: &[u8]) -> bool {
    if content.is_empty() {
        return true;
    }

    if let Ok(text) = std::str::from_utf8(content) {
        let printable = text
            .chars()
            .filter(|c| c.is_ascii_graphic() || c.is_ascii_whitespace() || c.is_control())
            .count();
        let total = text.chars().count();
        return printable > 0 && (printable as f64 / total as f64) > 0.8;
    }

    false
}

fn determine_upload_type(
    content: &[u8],
    filename: Option<&str>,
    from_clipboard: bool,
) -> (String, Option<String>, crate::models::UploadType) {
    if let Some(name) = filename {
        let ext = std::path::Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase());

        let text_extensions = [
            "txt", "md", "rs", "py", "js", "json", "toml", "yaml", "yml", "html", "css", "log",
            "xml", "csv", "ini", "conf", "sh", "bat",
        ];

        let image_extensions = [
            "png", "jpg", "jpeg", "gif", "webp", "svg", "ico", "bmp", "tif", "tiff",
        ];

        if let Some(ref ext) = ext {
            if text_extensions.contains(&ext.as_str()) {
                return (
                    "pastes".to_string(),
                    Some(name.to_string()),
                    crate::models::UploadType::Paste,
                );
            }
            if image_extensions.contains(&ext.as_str()) {
                return (
                    "images".to_string(),
                    Some(name.to_string()),
                    crate::models::UploadType::Image,
                );
            }
        }
    }

    // Special handling for clipboard content
    if from_clipboard {
        // Try to detect if clipboard contains image data by checking file extension
        if let Some(name) = filename {
            let image_extensions = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff"];

            let ext = std::path::Path::new(name)
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_lowercase());

            if let Some(ref ext) = ext {
                if image_extensions.contains(&ext.as_str()) {
                    return (
                        "images".to_string(),
                        Some(name.to_string()),
                        crate::models::UploadType::Image,
                    );
                }
            }
        }
    }

    if !is_binary_content(content) && is_definitely_text(content) {
        (
            "pastes".to_string(),
            filename.map(|s| s.to_string()),
            crate::models::UploadType::Paste,
        )
    } else {
        (
            "files".to_string(),
            filename.map(|s| s.to_string()),
            crate::models::UploadType::File,
        )
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let args = Args::parse();

    let (content, filename): (Vec<u8>, Option<String>) = if let Some(target_url) = &args.redirect {
        if !target_url.starts_with("http://") && !target_url.starts_with("https://") {
            anyhow::bail!("Redirect URL must start with http:// or https://");
        }
        let html_content = redirect_generator::generate_redirect_html(target_url);
        let filename = args.filename.clone();
        (html_content, filename)
    } else if args.clipboard {
        // Handle clipboard upload
        let clipboard_content =
            ClipboardContent::from_clipboard().context("Failed to read clipboard content")?;

        match clipboard_content {
            ClipboardContent::Text(text) => {
                let random_name = clipboard::generate_random_filename("txt");
                (text.into_bytes(), Some(random_name))
            }
            ClipboardContent::Image { data, format } => {
                let extension = clipboard::get_clipboard_extension(&format);
                let random_name = clipboard::generate_random_filename(extension);
                (data, Some(random_name))
            }
            ClipboardContent::Files(paths) => {
                if paths.len() == 1 {
                    // Single file from clipboard
                    let path = &paths[0];
                    let content = tokio::fs::read(path).await.with_context(|| {
                        format!("Failed to read file from clipboard path: {:?}", path)
                    })?;
                    let filename = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|s| s.to_string());
                    (content, filename)
                } else {
                    // Multiple files - create a text listing
                    let file_list = paths
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let random_name = clipboard::generate_random_filename("txt");
                    (file_list.into_bytes(), Some(random_name))
                }
            }
            ClipboardContent::Empty => {
                anyhow::bail!("Clipboard is empty");
            }
        }
    } else if let Some(file) = get_file_path(&args)? {
        let path = PathBuf::from(file);
        if !path.exists() {
            anyhow::bail!("File not found: {}", file);
        }

        let content = tokio::fs::read(&path)
            .await
            .with_context(|| format!("Failed to read file: {}", file))?;

        // Store extension for SFTP random filename generation with a prefix marker
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| format!("*.{}", s)); // * prefix means "use this extension with random name"

        (content, ext)
    } else if is_stdin_pipe() {
        let mut buffer = Vec::new();
        stdin()
            .read_to_end(&mut buffer)
            .await
            .context("Failed to read from stdin")?;

        if let Ok(text) = std::str::from_utf8(&buffer) {
            let trimmed = text.trim();
            if trimmed.starts_with("Error:")
                || trimmed.starts_with("error:")
                || trimmed.starts_with("Unknown option:")
                || trimmed.starts_with("unknown option:")
                || trimmed.starts_with("command not found")
                || trimmed.starts_with("Command not found")
                || trimmed.starts_with("usage:")
                || trimmed.starts_with("Usage:")
            {
                eprintln!("Error: Upstream command failed, not uploading error message");
                std::process::exit(1);
            }
        }

        if buffer.is_empty() {
            anyhow::bail!("No input received from stdin");
        }

        (buffer, None)
    } else {
        anyhow::bail!("No input provided. Use --file, --clipboard, or pipe data.");
    };

    let (detected_group, detected_filename, detected_upload_type) =
        determine_upload_type(&content, filename.as_deref(), args.clipboard);

    let is_redirect = args.redirect.is_some();
    let has_custom_filename = args.filename.is_some();
    let group = if is_redirect {
        args.group.unwrap_or_else(|| "pastes".to_string())
    } else {
        args.group.unwrap_or(detected_group)
    };
    let final_filename = if is_redirect && !has_custom_filename {
        None
    } else {
        args.filename.clone().or(detected_filename)
    };
    let upload_type = if is_redirect {
        crate::models::UploadType::Paste
    } else {
        detected_upload_type
    };

    let force_provider = args.provider.clone();

    let config = Arc::new(
        crate::config::Config::load()
            .with_context(|| "Failed to load config from ~/.config/pst/config.toml")?,
    );

    let should_strip_exif = !is_redirect && config.general.strip_exif && !args.no_exif;

    let processed_content = if upload_type == crate::models::UploadType::Image && should_strip_exif
    {
        match exif::strip_exif(&content) {
            Ok(stripped) => {
                eprintln!(
                    "Stripped EXIF metadata from image (original: {} bytes, stripped: {} bytes)",
                    content.len(),
                    stripped.len()
                );
                stripped
            }
            Err(e) => {
                eprintln!(
                    "Warning: Failed to strip EXIF ({}), using original image",
                    e
                );
                content
            }
        }
    } else {
        content
    };

    let request = crate::models::UploadRequest::new(
        processed_content,
        final_filename,
        upload_type,
        Some(crate::models::UploadOptions {
            expiration: args.expires,
            secret_url: false,
            custom_name: None,
        }),
        is_redirect,
    );

    let orchestrator = Arc::new(crate::orchestrator::UploadOrchestrator::new(config.clone()));

    let progress = orchestrator.create_progress_tracker(&request, "upload", args.progress);
    let progress_ref = progress.as_ref();

    let response = if let Some(provider_name) = force_provider {
        orchestrator
            .upload_to_specific_provider(&request, &provider_name, progress_ref)
            .await
    } else {
        orchestrator.upload(&request, &group, progress_ref).await
    };

    match args.output {
        OutputFormat::Url => {
            if let Some(url) = response.url {
                println!("{}", url);

                // Copy to clipboard if enabled
                let should_copy = args.copy_to_clipboard || config.general.copy_to_clipboard;
                if should_copy {
                    if let Err(e) = copy_to_clipboard(&url) {
                        eprintln!("Warning: Failed to copy to clipboard: {}", e);
                    } else {
                        eprintln!("URL copied to clipboard");
                    }
                }
            } else {
                eprintln!(
                    "Error: {}",
                    response
                        .error
                        .unwrap_or_else(|| "Unknown error".to_string())
                );
                std::process::exit(1);
            }
        }
        OutputFormat::Json => {
            let json_output = serde_json::json!({
                "success": response.success,
                "url": response.url,
                "provider": response.provider,
                "error": response.error,
            });
            println!("{}", serde_json::to_string_pretty(&json_output)?);
        }
        OutputFormat::Verbose => {
            println!("{:#?}", response);
        }
    }

    Ok(())
}
