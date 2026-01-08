use anyhow::{Context, Result};
use arboard::Clipboard;
use image::ImageBuffer;
use rand::Rng;
use std::path::PathBuf;

#[derive(Debug)]
pub enum ClipboardContent {
    Text(String),
    Image { data: Vec<u8>, format: ImageFormat },
    Files(Vec<PathBuf>),
    Empty,
}

#[derive(Debug, Clone)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Bmp,
    Tiff,
    WebP,
    Unknown,
}

impl ClipboardContent {
    pub fn from_clipboard() -> Result<Self> {
        let mut clipboard = Clipboard::new().context("Failed to access clipboard")?;

        // Try to get image data first
        if let Ok(image_data) = clipboard.get_image() {
            let bytes_per_pixel = image_data.bytes.len() / (image_data.width * image_data.height);

            let result = match bytes_per_pixel {
                4 => {
                    let img: ImageBuffer<image::Rgba<u8>, Vec<u8>> = ImageBuffer::from_vec(
                        image_data.width as u32,
                        image_data.height as u32,
                        image_data.bytes.to_vec(),
                    )
                    .context("Failed to create image buffer")?;
                    let mut cursor = std::io::Cursor::new(Vec::new());
                    img.write_to(&mut cursor, image::ImageFormat::Png)
                        .context("Failed to encode image as PNG")?;
                    Ok(ClipboardContent::Image {
                        data: cursor.into_inner(),
                        format: ImageFormat::Png,
                    })
                }
                3 => {
                    let img: ImageBuffer<image::Rgb<u8>, Vec<u8>> = ImageBuffer::from_vec(
                        image_data.width as u32,
                        image_data.height as u32,
                        image_data.bytes.to_vec(),
                    )
                    .context("Failed to create image buffer")?;
                    let mut cursor = std::io::Cursor::new(Vec::new());
                    img.write_to(&mut cursor, image::ImageFormat::Png)
                        .context("Failed to encode image as PNG")?;
                    Ok(ClipboardContent::Image {
                        data: cursor.into_inner(),
                        format: ImageFormat::Png,
                    })
                }
                1 => {
                    let img: ImageBuffer<image::Luma<u8>, Vec<u8>> = ImageBuffer::from_vec(
                        image_data.width as u32,
                        image_data.height as u32,
                        image_data.bytes.to_vec(),
                    )
                    .context("Failed to create image buffer")?;
                    let mut cursor = std::io::Cursor::new(Vec::new());
                    img.write_to(&mut cursor, image::ImageFormat::Png)
                        .context("Failed to encode image as PNG")?;
                    Ok(ClipboardContent::Image {
                        data: cursor.into_inner(),
                        format: ImageFormat::Png,
                    })
                }
                _ => {
                    let img: ImageBuffer<image::Rgba<u8>, Vec<u8>> = ImageBuffer::from_vec(
                        image_data.width as u32,
                        image_data.height as u32,
                        image_data.bytes.to_vec(),
                    )
                    .context("Failed to create image buffer")?;
                    let mut cursor = std::io::Cursor::new(Vec::new());
                    img.write_to(&mut cursor, image::ImageFormat::Png)
                        .context("Failed to encode image as PNG")?;
                    Ok(ClipboardContent::Image {
                        data: cursor.into_inner(),
                        format: ImageFormat::Png,
                    })
                }
            };

            return result;
        }

        // Try to get text
        if let Ok(text) = clipboard.get_text() {
            if !text.trim().is_empty() {
                if text.len() > 100 && is_likely_binary_data(text.as_bytes()) {
                    let format = detect_image_format(text.as_bytes());
                    return Ok(ClipboardContent::Image {
                        data: text.into_bytes(),
                        format,
                    });
                }

                let paths: Vec<PathBuf> = text
                    .split(&['\n', '\r', ' '][..])
                    .filter(|s| !s.trim().is_empty())
                    .map(PathBuf::from)
                    .filter(|p| p.exists())
                    .collect();

                if !paths.is_empty() {
                    return Ok(ClipboardContent::Files(paths));
                }

                return Ok(ClipboardContent::Text(text));
            }
        }

        Ok(ClipboardContent::Empty)
    }
}

fn detect_image_format(data: &[u8]) -> ImageFormat {
    if data.len() < 8 {
        return ImageFormat::Unknown;
    }

    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return ImageFormat::Png;
    }

    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return ImageFormat::Jpeg;
    }

    if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        return ImageFormat::Gif;
    }

    if data.starts_with(b"BM") {
        return ImageFormat::Bmp;
    }

    if data.len() >= 12 && data.starts_with(b"RIFF") && data[8..12] == *b"WEBP" {
        return ImageFormat::WebP;
    }

    if (data.starts_with(&[0x4D, 0x4D, 0x00, 0x2A]) || data.starts_with(&[0x49, 0x49, 0x2A, 0x00]))
        && data.len() >= 4
    {
        return ImageFormat::Tiff;
    }

    ImageFormat::Unknown
}

fn is_likely_binary_data(data: &[u8]) -> bool {
    if data.len() > 4 {
        if data.starts_with(&[0xFF, 0xD8, 0xFF])
            || data.starts_with(&[0x89, 0x50, 0x4E, 0x47])
            || data.starts_with(b"GIF87a")
            || data.starts_with(b"GIF89a")
            || data.starts_with(b"%PDF")
        {
            return true;
        }

        let text_chars = data
            .iter()
            .filter(|&&b| {
                b.is_ascii_graphic()
                    || b.is_ascii_whitespace()
                    || b == b'\n'
                    || b == b'\r'
                    || b == b'\t'
            })
            .count();

        let text_ratio = text_chars as f64 / data.len() as f64;
        return text_ratio < 0.8;
    }
    false
}

pub fn get_clipboard_extension(format: &ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Gif => "gif",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Tiff => "tiff",
        ImageFormat::WebP => "webp",
        ImageFormat::Unknown => "bin",
    }
}

pub fn generate_random_filename(extension: &str) -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::thread_rng();
    let random: String = (0..8)
        .map(|_| {
            let idx = rng.gen::<usize>() % CHARSET.len();
            CHARSET[idx] as char
        })
        .collect();
    let ext = if extension.starts_with('.') {
        extension
    } else {
        &format!(".{}", extension)
    };
    format!("{}{}", random, ext)
}
