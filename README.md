# PST - Paste and Share Tool

A Rust command-line application for uploading files and pastes to multiple sharing services with automatic prioritization and fallback.

## Features

- **Multi-provider support**: Upload to 0x0.st, paste.rs, uguu.se, and FTP/SFTP
- **Automatic fallback**: If one provider fails, automatically tries the next one
- **Smart content detection**: Automatically detects text pastes vs binary files
- **Priority system**: Configure which providers to try first
- **Progress tracking**: Optional progress bar for large uploads
- **Multiple output formats**: URL, JSON, or verbose output
- **Configuration file**: All settings in `~/.config/pst/config.toml`

## Installation

```bash
# Build from source
cd pst
cargo build --release

# Copy binary to PATH
cp target/release/pst /usr/local/bin/
```

## First Run

On first run, a default configuration file is created automatically at:
- **Linux:** `~/.config/pst/config.toml`
- **macOS:** `~/Library/Application Support/pst/config.toml`
- **Windows:** `%APPDATA%\pst\config.toml`

The default config includes all HTTP providers enabled. FTP/SFTP is disabled by default (requires configuration).

## Usage

### Upload a file
```bash
pst document.pdf
pst image.png
```

### Upload via pipe
```bash
echo "Hello, World!" | pst
cat file.txt | pst
cat archive.zip | pst
```

### Options
```bash
-f, --file <FILE>          File to upload
-n, --filename <FILENAME>  Custom filename for the upload
-o, --output <FORMAT>      Output format [default: url] [possible values: url, json, verbose]
-g, --group <GROUP>        Provider group to use (files, pastes, images)
-p, --provider <PROVIDER>  Force specific provider
-e, --expires <EXPIRES>    Set expiration time
    --progress             Show progress bar
-h, --help                 Print help
-V, --version              Print version
```

### Force Specific Provider
```bash
# Force upload to a specific provider
pst document.pdf --provider 0x0st
pst image.png -p uguu
echo "text" | pst --provider paste_rs

# Available providers:
# 0x0st, paste_rs, uguu, ftp_sftp
```

### Custom Filename
```bash
# Use a custom filename for the upload
pst document.pdf -n myfile.pdf
echo "content" | pst --filename custom.txt
```

### Provider Order

Providers are tried in the order specified in `[provider_groups]` in your config file. For example:

```toml
[provider_groups.files]
providers = ["0x0st", "uguu", "ftp_sftp"]
```

This means 0x0st will be tried first, then uguu.se, and so on until one succeeds.

### Force Specific Provider
```bash
# Force upload to a specific provider
pst document.pdf --provider 0x0st
pst image.png --provider uguu
echo "text" | pst --provider paste_rs

# Available providers:
# 0x0st, paste_rs, uguu, ftp_sftp
```

## Configuration

Create `~/.config/pst/config.toml`:

```toml
[general]
timeout_seconds = 30
max_retries = 3
retry_delay_ms = 1000

[providers.0x0st]
enabled = true
priority = 1

[providers.paste_rs]
enabled = true
priority = 2

[providers.uguu]
enabled = true
priority = 3

[providers.ftp_sftp]
enabled = true
priority = 5
host = "ftp.example.com"
port = 22
username = "user"
password = "pass"
directory = "/public_html/uploads"
public_url = "https://cdn.example.com/uploads"
enable_sftp = true

[provider_groups]
files = ["0x0st", "uguu", "ftp_sftp"]
pastes = ["paste_rs", "ftp_sftp"]
images = ["0x0st", "uguu", "ftp_sftp"]
```

## Available Providers

| Provider | Type | Max Size | Features |
|----------|------|----------|----------|
| `0x0st` | Files | 512 MiB | Secret URLs, expiration |
| `paste_rs` | Pastes | ~10 MiB | Syntax highlighting |
| `uguu` | Files | 128 MiB | 3-hour retention |
| `ftp_sftp` | Files | Configurable | Custom public URL |

## Force Specific Provider

Use `-p` or `--provider` to force upload to a specific provider:

```bash
# Force upload to a specific provider
pst document.pdf --provider 0x0st
pst image.png -p uguu
echo "text content" | pst --provider paste_rs

# Combine with other options
pst large_file.zip -p uguu --output json --progress
```

## How It Works

1. **Content Detection**: Automatically detects if input is text or binary
2. **Provider Selection**: Uses configured priority order or explicit group
3. **Upload Attempt**: Tries each provider in order
4. **Fallback**: On failure, automatically tries next provider
5. **Success**: Returns URL from first successful provider

## Examples

```bash
# Upload text paste (auto-detected)
echo "Hello World" | pst
# Output: https://paste.rs/abc123

# Upload file with progress
pst large_video.mp4 --progress
# Output: https://uguu.se/xyz789.mp4

# Force specific provider
pst document.pdf --provider 0x0st
# Output: https://0x0.st/def456.pdf

# JSON output for scripting
pst data.json --output json
# Output: {"success":true,"url":"https://0x0.st/..."}
```

## Requirements

- Rust 1.70+
- OpenSSL (for SFTP)
- Configuration file at `~/.config/pst/config.toml`

## License

MIT
