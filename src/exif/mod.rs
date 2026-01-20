use anyhow::{Context, Result};
use image::ImageFormat;
use std::io::{Read, Seek, SeekFrom, Write};

pub fn strip_exif(data: &[u8]) -> Result<Vec<u8>> {
    let format = detect_format(data)?;

    match format {
        ImageFormat::Jpeg => strip_jpeg_exif(data),
        ImageFormat::Png => strip_png_exif(data),
        ImageFormat::WebP => strip_webp_exif(data),
        _ => strip_generic(data, format),
    }
}

fn detect_format(data: &[u8]) -> Result<ImageFormat> {
    if data.len() < 8 {
        return Err(anyhow::anyhow!("Data too short to detect format"));
    }

    let format = image::guess_format(data).context("Failed to detect image format")?;

    Ok(format)
}

fn read_u8<R: Read>(reader: &mut R) -> Result<u8> {
    let mut byte = [0u8; 1];
    reader.read_exact(&mut byte)?;
    Ok(byte[0])
}

fn read_u16<R: Read>(reader: &mut R) -> Result<u16> {
    let mut bytes = [0u8; 2];
    reader.read_exact(&mut bytes)?;
    Ok(u16::from_be_bytes(bytes))
}

fn read_u32<R: Read>(reader: &mut R) -> Result<u32> {
    let mut bytes = [0u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn copy<R: Read, W: Write>(reader: &mut R, writer: &mut W, mut n: u64) -> Result<u64> {
    let mut buffer = vec![0u8; 8192];
    let mut total = 0u64;

    while n > 0 {
        let to_read = std::cmp::min(buffer.len() as u64, n);
        let bytes_read = reader.read(&mut buffer[..to_read as usize])?;

        if bytes_read == 0 {
            break;
        }

        writer.write_all(&buffer[..bytes_read])?;
        total += bytes_read as u64;
        n -= bytes_read as u64;
    }

    Ok(total)
}

fn skip<R: Seek>(reader: &mut R, n: i64) -> Result<()> {
    reader.seek(SeekFrom::Current(n))?;
    Ok(())
}

fn strip_jpeg_exif(data: &[u8]) -> Result<Vec<u8>> {
    let mut source = std::io::Cursor::new(data);
    let mut destination = Vec::new();

    loop {
        let mut marker = [0u8; 2];
        if source.read_exact(&mut marker).is_err() {
            break;
        }

        match marker[1] {
            0xC0..=0xCF => {
                copy_jpeg_header(&mut source, &mut destination, &marker)?;
            }
            0xD0..=0xD7 => {
                destination.write_all(&marker)?;
                copy_jpeg_data(&mut source, &mut destination)?;
            }
            0xD8..=0xD9 => {
                destination.write_all(&marker)?;
            }
            0xDA => {
                copy_jpeg_header(&mut source, &mut destination, &marker)?;
                copy_jpeg_data(&mut source, &mut destination)?;
            }
            0xDB..=0xDF => {
                copy_jpeg_header(&mut source, &mut destination, &marker)?;
            }
            0xE0 => {
                let size = read_u16(&mut source)?;
                if size >= 16 {
                    let mut identifier = [0u8; 5];
                    source.read_exact(&mut identifier)?;
                    if identifier == *b"JFIF\0" {
                        destination
                            .write_all(&[0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00])?;
                        copy(&mut source, &mut destination, 7)?;
                        destination.write_all(&[0x00, 0x00])?;
                        skip(&mut source, size as i64 - 14)?;
                    } else {
                        skip(&mut source, size as i64 - 7)?;
                    }
                } else {
                    skip(&mut source, size as i64 - 2)?;
                }
            }
            _ => {
                let size = read_u16(&mut source)?;
                skip(&mut source, size as i64 - 2)?;
            }
        }
    }

    Ok(destination)
}

fn copy_jpeg_header<R: Read, W: Write>(
    source: &mut R,
    destination: &mut W,
    marker: &[u8; 2],
) -> Result<()> {
    destination.write_all(marker)?;
    let size = read_u16(source)?;
    destination.write_all(&size.to_be_bytes())?;
    if size > 2 {
        let size = size as u64 - 2;
        copy(source, destination, size)?;
    }
    Ok(())
}

fn copy_jpeg_data<R: Read + Seek, W: Write>(source: &mut R, destination: &mut W) -> Result<()> {
    loop {
        let value = read_u8(source)?;
        if value == 0xFF {
            let next = read_u8(source)?;
            if next == 0 {
                destination.write_all(&[value, next])?;
            } else {
                source.seek(SeekFrom::Current(-2))?;
                break;
            }
        } else {
            destination.write_all(&[value])?;
        }
    }
    Ok(())
}

fn strip_png_exif(data: &[u8]) -> Result<Vec<u8>> {
    let mut source = std::io::Cursor::new(data);
    let mut destination = Vec::new();

    if copy(&mut source, &mut destination, 8)? < 8 {
        return Err(anyhow::anyhow!("Invalid PNG file"));
    }

    loop {
        let mut size_bytes = [0u8; 4];
        if source.read_exact(&mut size_bytes).is_err() {
            break;
        }
        let size = u32::from_be_bytes(size_bytes);

        let mut chunk = [0u8; 4];
        if source.read_exact(&mut chunk).is_err() {
            return Err(anyhow::anyhow!("Invalid PNG chunk"));
        }

        match &chunk {
            b"IDAT" | b"IEND" | b"IHDR" | b"PLTE" | b"acTL" | b"bKGD" | b"cHRM" | b"cICP"
            | b"fRAc" | b"fcTL" | b"fdAT" | b"gAMA" | b"gIFg" | b"iCCP" | b"sBIT" | b"sRGB"
            | b"sTER" | b"tRNS" => {
                destination.write_all(&size.to_be_bytes())?;
                destination.write_all(&chunk)?;
                let chunk_size = size as u64 + 4;
                copy(&mut source, &mut destination, chunk_size)?;
            }
            _ => {
                skip(&mut source, size as i64 + 4)?;
            }
        }
    }

    Ok(destination)
}

fn strip_webp_exif(data: &[u8]) -> Result<Vec<u8>> {
    let mut source = std::io::Cursor::new(data);
    let mut destination = Vec::new();

    source.seek(SeekFrom::Current(8))?;

    let mut code = [0u8; 4];
    source.read_exact(&mut code)?;
    if &code != b"WEBP" {
        return Err(anyhow::anyhow!("Not a valid WebP file"));
    }

    let mut webp_data = Vec::new();
    while webp_chunk(&mut source, &mut webp_data)? {}

    let size = webp_data.len() as u32 + 4;
    destination.write_all(b"RIFF")?;
    destination.write_all(&size.to_le_bytes())?;
    destination.write_all(b"WEBP")?;
    destination.write_all(&webp_data)?;

    Ok(destination)
}

fn webp_chunk<R: Read + Seek, W: Write>(source: &mut R, destination: &mut W) -> Result<bool> {
    let mut code = [0u8; 4];
    if source.read_exact(&mut code).is_err() {
        return Ok(false);
    }

    let size = read_u32(source)?;
    let total_size = if size % 2 > 0 { size + 1 } else { size };

    match &code {
        b"ALPH" | b"ANIM" | b"ANMF" | b"VP8 " | b"VP8L" | b"VP8X" => {
            destination.write_all(&code)?;
            destination.write_all(&size.to_le_bytes())?;
            copy(source, destination, total_size as u64)?;
        }
        _ => {
            skip(source, total_size as i64)?;
        }
    }

    Ok(true)
}

fn strip_generic(data: &[u8], format: ImageFormat) -> Result<Vec<u8>> {
    let img = image::load_from_memory(data).context("Failed to load image")?;

    let mut buffer = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buffer, format)
        .context("Failed to re-encode image")?;

    Ok(buffer.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jpeg_exif_removal() {
        let jpeg_with_exif: Vec<u8> = vec![
            0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x0C, b'E', b'x', b'i', b'f', 0x00, 0x00, b't', b'e',
            b's', b't', 0xFF, 0xD9,
        ];

        let result = strip_jpeg_exif(&jpeg_with_exif).unwrap();

        assert!(!result.contains(&0xE1), "Should not contain EXIF marker");
        assert!(
            result.starts_with(&[0xFF, 0xD8]),
            "Should start with JPEG SOI"
        );
        assert!(result.ends_with(&[0xFF, 0xD9]), "Should end with JPEG EOI");
    }

    #[test]
    fn test_jpeg_without_exif_unchanged() {
        let jpeg_no_exif = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0x00, 0x01, 0x01, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xD9,
        ];

        let result = strip_jpeg_exif(&jpeg_no_exif).unwrap();

        assert_eq!(result.len(), jpeg_no_exif.len());
        assert_eq!(result, jpeg_no_exif.to_vec());
    }

    #[test]
    fn test_png_exif_removal() {
        let mut png_with_exif = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

        let exif_chunk = create_png_chunk(b"tEXt", b"Exif\x00some exif data");
        png_with_exif.extend_from_slice(&exif_chunk);

        let ihdr_chunk = create_png_chunk(b"IHDR", b"width\x00\x00\x00\x01");
        png_with_exif.extend_from_slice(&ihdr_chunk);

        let iend_chunk = create_png_chunk(b"IEND", &[]);
        png_with_exif.extend_from_slice(&iend_chunk);

        let result = strip_png_exif(&png_with_exif).unwrap();

        let result_str = String::from_utf8_lossy(&result);
        assert!(
            !result_str.contains("Exif"),
            "Should not contain EXIF keyword"
        );
    }

    fn create_png_chunk(chunk_type: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut chunk = Vec::new();

        let len = (data.len() as u32).to_be_bytes();
        chunk.extend_from_slice(&len);
        chunk.extend_from_slice(chunk_type);
        chunk.extend_from_slice(data);

        let crc = calculate_crc(&chunk[4..]);
        chunk.extend_from_slice(&crc.to_be_bytes());

        chunk
    }

    fn calculate_crc(_data: &[u8]) -> u32 {
        0
    }
}
