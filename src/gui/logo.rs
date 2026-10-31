//! Application logo.
//!
//! The artwork is embedded in the binary so packaged builds do not need to
//! ship a separate asset file.

use eframe::egui::{self, ColorImage, TextureOptions};
use std::sync::OnceLock;

/// Source artwork, compiled into the executable.
const LOGO_JPEG: &[u8] = include_bytes!("../../pictures/frequency.jpg");

/// Logo reduced to a sensible texture size. The original is 1024×1024, which
/// is far larger than the 36 px header image or the window icon need.
const LOGO_SIZE: u32 = 256;

struct DecodedLogo {
    rgba: Vec<u8>,
    size: [usize; 2],
}

fn decode() -> Option<DecodedLogo> {
    let image = match image::load_from_memory_with_format(LOGO_JPEG, image::ImageFormat::Jpeg) {
        Ok(image) => image,
        Err(error) => {
            log::warn!("embedded logo could not be decoded: {error}");
            return None;
        }
    };

    let image = image.thumbnail(LOGO_SIZE, LOGO_SIZE).to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Some(DecodedLogo {
        rgba: image.into_raw(),
        size,
    })
}

fn decoded() -> Option<&'static DecodedLogo> {
    static LOGO: OnceLock<Option<DecodedLogo>> = OnceLock::new();
    LOGO.get_or_init(decode).as_ref()
}

/// Upload the logo to the GPU for use in the header bar.
pub fn texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let logo = decoded()?;
    let image = ColorImage::from_rgba_unmultiplied(logo.size, &logo.rgba);
    Some(ctx.load_texture("frequency-logo", image, TextureOptions::LINEAR))
}

/// The logo as a window/taskbar icon.
pub fn icon_data() -> Option<egui::IconData> {
    let logo = decoded()?;
    Some(egui::IconData {
        rgba: logo.rgba.clone(),
        width: logo.size[0] as u32,
        height: logo.size[1] as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_logo_decodes_to_rgba() {
        let logo = decoded().expect("embedded logo should decode");
        assert!(logo.size[0] > 0 && logo.size[1] > 0);
        assert!(logo.size[0] <= LOGO_SIZE as usize && logo.size[1] <= LOGO_SIZE as usize);
        assert_eq!(logo.rgba.len(), logo.size[0] * logo.size[1] * 4);
    }

    #[test]
    fn icon_data_matches_the_logo() {
        let logo = decoded().expect("embedded logo should decode");
        let icon = icon_data().expect("embedded logo should produce icon data");
        assert_eq!(icon.width as usize, logo.size[0]);
        assert_eq!(icon.height as usize, logo.size[1]);
        assert_eq!(
            icon.rgba.len(),
            icon.width as usize * icon.height as usize * 4
        );
    }
}
