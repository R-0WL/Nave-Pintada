use anyhow::{Context, Result};
use std::path::Path;

use crate::model::Scene;

pub fn save_scene(scene: &Scene, path: &Path) -> Result<()> {
    let file = std::fs::File::create(path).with_context(|| format!("No se pudo crear {}", path.display()))?;
    serde_json::to_writer_pretty(file, scene).context("No se pudo serializar el proyecto")?;
    Ok(())
}

pub fn load_scene(path: &Path) -> Result<Scene> {
    let file = std::fs::File::open(path).with_context(|| format!("No se pudo abrir {}", path.display()))?;
    let scene: Scene = serde_json::from_reader(file).context("El archivo no es un proyecto R3D válido")?;
    Ok(scene)
}

pub fn save_screenshot(buffer: &[u32], width: usize, height: usize, path: &Path) -> Result<()> {
    let mut rgba = Vec::with_capacity(width * height * 4);
    for &pixel in buffer {
        rgba.push(((pixel >> 16) & 0xff) as u8);
        rgba.push(((pixel >> 8) & 0xff) as u8);
        rgba.push((pixel & 0xff) as u8);
        rgba.push(255);
    }
    let image = image::RgbaImage::from_raw(width as u32, height as u32, rgba)
        .context("No se pudo construir la imagen de captura")?;
    image.save(path).with_context(|| format!("No se pudo guardar la captura en {}", path.display()))?;
    Ok(())
}
