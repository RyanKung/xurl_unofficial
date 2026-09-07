//! Media type detection and upload limits for tweet attachments.

use std::path::Path;

use crate::error::Error;

/// Max bytes accepted for a single upload (video-sized).
pub const MAX_BYTES: usize = 15_000_000;

/// APPEND segment size.
pub const SEGMENT_BYTES: usize = 1_048_576;

/// MIME type and X `media_category` for a local file.
pub fn kind_for_path(path: &Path) -> Result<(&'static str, &'static str), Error> {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .ok_or(Error::InvalidMedia)?;
    match ext.as_str() {
        "jpg" | "jpeg" => Ok(("image/jpeg", "tweet_image")),
        "png" => Ok(("image/png", "tweet_image")),
        "webp" => Ok(("image/webp", "tweet_image")),
        "gif" => Ok(("image/gif", "tweet_gif")),
        "mp4" => Ok(("video/mp4", "tweet_video")),
        _ => Err(Error::InvalidMedia),
    }
}

/// Reject empty or oversized payloads.
pub fn validate_bytes(bytes: &[u8]) -> Result<(), Error> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        Err(Error::InvalidMedia)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::kind_for_path;
    use std::path::Path;

    #[test]
    fn jpeg_and_mp4_map_to_categories() {
        let jpeg = kind_for_path(Path::new("shot.JPG"));
        assert_eq!(jpeg.ok(), Some(("image/jpeg", "tweet_image")));
        let video = kind_for_path(Path::new("clip.mp4"));
        assert_eq!(video.ok(), Some(("video/mp4", "tweet_video")));
        assert!(kind_for_path(Path::new("notes.txt")).is_err());
    }
}
