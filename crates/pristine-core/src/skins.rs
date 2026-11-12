//! A small local skin library, plus uploading a skin to a Microsoft account.

use crate::error::{Error, Result};
use crate::instance::slugify;
use crate::paths::Paths;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;

const MAX_BYTES: usize = 200_000;

#[derive(Debug, Serialize)]
pub struct SkinEntry {
    pub file: String,
    pub name: String,
    /// PNG, base64-encoded.
    pub data: String,
}

/// A valid skin is a PNG of 64x64 or 64x32 pixels.
pub fn validate_png(b: &[u8]) -> Result<()> {
    if b.len() > MAX_BYTES { return Err(Error::msg("That skin file is too large.")); }
    if b.len() < 24 || &b[..8] != b"\x89PNG\r\n\x1a\n" || &b[12..16] != b"IHDR" {
        return Err(Error::msg("Skins must be PNG images."));
    }
    let (w, h) = (u32::from_be_bytes(b[16..20].try_into().unwrap()), u32::from_be_bytes(b[20..24].try_into().unwrap()));
    if w == 64 && (h == 64 || h == 32) { Ok(()) } else {
        Err(Error::msg(format!("Skins must be 64×64 (or 64×32) pixels, this one is {w}×{h}.")))
    }
}

fn safe_file(file: &str) -> Result<&str> {
    if file.ends_with(".png") && !file.contains(['/', '\\']) && !file.starts_with('.') { Ok(file) }
    else { Err(Error::msg("invalid skin file name")) }
}

pub fn list(paths: &Paths) -> Vec<SkinEntry> {
    let Ok(rd) = std::fs::read_dir(paths.skins()) else { return vec![] };
    let mut out: Vec<SkinEntry> = rd.flatten().filter_map(|e| {
        let file = e.file_name().to_string_lossy().into_owned();
        let bytes = std::fs::read(e.path()).ok()?;
        validate_png(&bytes).ok()?;
        Some(SkinEntry { name: file.trim_end_matches(".png").replace('-', " "), data: STANDARD.encode(bytes), file })
    }).collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Save a base64 PNG under a name derived from `name`; returns the file name.
pub fn save(paths: &Paths, name: &str, b64: &str) -> Result<String> {
    let bytes = STANDARD.decode(b64.trim()).map_err(|_| Error::msg("Couldn't read that image."))?;
    validate_png(&bytes)?;
    std::fs::create_dir_all(paths.skins())?;
    let file = format!("{}.png", slugify(name));
    std::fs::write(paths.skins().join(&file), bytes)?;
    Ok(file)
}

pub fn read(paths: &Paths, file: &str) -> Result<Vec<u8>> {
    Ok(std::fs::read(paths.skins().join(safe_file(file)?))?)
}

pub fn delete(paths: &Paths, file: &str) -> Result<()> {
    let _ = std::fs::remove_file(paths.skins().join(safe_file(file)?));
    Ok(())
}

pub async fn apply(client: &reqwest::Client, mc_token: &str, png: Vec<u8>, slim: bool) -> Result<()> {
    validate_png(&png)?;
    let form = reqwest::multipart::Form::new()
        .text("variant", if slim { "slim" } else { "classic" })
        .part("file", reqwest::multipart::Part::bytes(png).file_name("skin.png").mime_str("image/png")?);
    let r = client.post("https://api.minecraftservices.com/minecraft/profile/skins")
        .bearer_auth(mc_token).multipart(form).send().await?;
    if r.status().is_success() { Ok(()) } else {
        Err(Error::msg(format!("Minecraft rejected the skin (HTTP {})", r.status())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
        b.extend_from_slice(&13u32.to_be_bytes());
        b.extend_from_slice(b"IHDR");
        b.extend_from_slice(&w.to_be_bytes());
        b.extend_from_slice(&h.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        b
    }

    #[test]
    fn validates_dimensions_and_magic() {
        assert!(validate_png(&png(64, 64)).is_ok());
        assert!(validate_png(&png(64, 32)).is_ok());
        assert!(validate_png(&png(128, 128)).is_err());
        assert!(validate_png(b"not a png at all, definitely not").is_err());
    }

    #[test]
    fn save_list_delete_roundtrip() {
        let dir = std::env::temp_dir().join(format!("pristine-skin-{}", uuid::Uuid::new_v4()));
        let paths = Paths::new(&dir);
        let file = save(&paths, "My Cool Skin", &STANDARD.encode(png(64, 64))).unwrap();
        assert_eq!(file, "my-cool-skin.png");
        assert_eq!(list(&paths)[0].name, "my cool skin");
        assert!(read(&paths, "../x.png").is_err());
        delete(&paths, &file).unwrap();
        assert!(list(&paths).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
