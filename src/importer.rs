use anyhow::{anyhow, Context, Result};
use std::path::Path;

use crate::model::{EditorCamera, Material, Mesh, Object3D, Scene, TextureAsset, Triangle, Vertex};

fn texture_from_path(path: &Path) -> Option<TextureAsset> {
    let img = image::open(path).ok()?.to_rgba8();
    Some(TextureAsset { width: img.width(), height: img.height(), pixels: img.into_raw() })
}

fn rgba_from_gltf_image(data: &gltf::image::Data) -> Option<TextureAsset> {
    use gltf::image::Format;
    let count = (data.width as usize).checked_mul(data.height as usize)?;
    let mut out = Vec::with_capacity(count * 4);
    match data.format {
        Format::R8 => for &r in data.pixels.iter().take(count) { out.extend_from_slice(&[r, r, r, 255]); },
        Format::R8G8 => for p in data.pixels.chunks_exact(2).take(count) { out.extend_from_slice(&[p[0], p[0], p[0], p[1]]); },
        Format::R8G8B8 => for p in data.pixels.chunks_exact(3).take(count) { out.extend_from_slice(&[p[0], p[1], p[2], 255]); },
        Format::R8G8B8A8 => out.extend_from_slice(&data.pixels[..count.saturating_mul(4).min(data.pixels.len())]),
        Format::R16 => for p in data.pixels.chunks_exact(2).take(count) { let v = u16::from_le_bytes([p[0], p[1]]) >> 8; let v = v as u8; out.extend_from_slice(&[v, v, v, 255]); },
        Format::R16G16 => for p in data.pixels.chunks_exact(4).take(count) { let gray = (u16::from_le_bytes([p[0], p[1]]) >> 8) as u8; let alpha = (u16::from_le_bytes([p[2], p[3]]) >> 8) as u8; out.extend_from_slice(&[gray, gray, gray, alpha]); },
        Format::R16G16B16 => for p in data.pixels.chunks_exact(6).take(count) { let r = (u16::from_le_bytes([p[0], p[1]]) >> 8) as u8; let g = (u16::from_le_bytes([p[2], p[3]]) >> 8) as u8; let b = (u16::from_le_bytes([p[4], p[5]]) >> 8) as u8; out.extend_from_slice(&[r, g, b, 255]); },
        Format::R16G16B16A16 => for p in data.pixels.chunks_exact(8).take(count) { let r = (u16::from_le_bytes([p[0], p[1]]) >> 8) as u8; let g = (u16::from_le_bytes([p[2], p[3]]) >> 8) as u8; let b = (u16::from_le_bytes([p[4], p[5]]) >> 8) as u8; let a = (u16::from_le_bytes([p[6], p[7]]) >> 8) as u8; out.extend_from_slice(&[r, g, b, a]); },
        Format::R32G32B32FLOAT => for p in data.pixels.chunks_exact(12).take(count) { let r = f32::from_le_bytes(p[0..4].try_into().ok()?).clamp(0.0, 1.0); let g = f32::from_le_bytes(p[4..8].try_into().ok()?).clamp(0.0, 1.0); let b = f32::from_le_bytes(p[8..12].try_into().ok()?).clamp(0.0, 1.0); out.extend_from_slice(&[(r*255.0) as u8,(g*255.0) as u8,(b*255.0) as u8,255]); },
        Format::R32G32B32A32FLOAT => for p in data.pixels.chunks_exact(16).take(count) { let r = f32::from_le_bytes(p[0..4].try_into().ok()?).clamp(0.0, 1.0); let g = f32::from_le_bytes(p[4..8].try_into().ok()?).clamp(0.0, 1.0); let b = f32::from_le_bytes(p[8..12].try_into().ok()?).clamp(0.0, 1.0); let a = f32::from_le_bytes(p[12..16].try_into().ok()?).clamp(0.0, 1.0); out.extend_from_slice(&[(r*255.0) as u8,(g*255.0) as u8,(b*255.0) as u8,(a*255.0) as u8]); },
    }
    if out.len() < count * 4 { return None; }
    Some(TextureAsset { width: data.width, height: data.height, pixels: out })
}

pub fn load_model(path: &Path) -> Result<Vec<Object3D>> {
    match path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "obj" => load_obj(path),
        "gltf" | "glb" => load_gltf(path),
        ext => Err(anyhow!("Formato no soportado: .{ext}")),
    }
}

fn load_obj(path: &Path) -> Result<Vec<Object3D>> {
    let options = tobj::LoadOptions { triangulate: true, single_index: true, ignore_points: true, ignore_lines: true, ..Default::default() };
    let (models, material_result) = tobj::load_obj(path, &options).context("No se pudo leer el OBJ")?;
    let imported_materials = material_result.unwrap_or_default();
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut materials = Vec::new();
    for mat in &imported_materials {
        let diffuse = mat.diffuse.unwrap_or([0.72, 0.74, 0.78]);
        let texture_source = mat.diffuse_texture.clone();
        let texture_path = texture_source.as_ref().map(|p| base_dir.join(p));
        materials.push(Material {
            name: mat.name.clone(),
            base_color: [
                (diffuse[0].clamp(0.0, 1.0) * 255.0) as u8,
                (diffuse[1].clamp(0.0, 1.0) * 255.0) as u8,
                (diffuse[2].clamp(0.0, 1.0) * 255.0) as u8,
                255,
            ],
            texture: texture_path.as_deref().and_then(texture_from_path),
            texture_source: texture_path.map(|p| p.to_string_lossy().to_string()),
        });
    }
    if materials.is_empty() { materials.push(Material::default()); }

    let mut objects = Vec::new();
    for model in models {
        let m = model.mesh;
        let mut vertices = Vec::with_capacity(m.positions.len() / 3);
        for i in 0..m.positions.len() / 3 {
            let uv = if m.texcoords.len() >= (i + 1) * 2 { [m.texcoords[i * 2], m.texcoords[i * 2 + 1]] } else { [0.0, 0.0] };
            vertices.push(Vertex { position: [m.positions[i * 3], m.positions[i * 3 + 1], m.positions[i * 3 + 2]], uv });
        }
        let material_id = m.material_id.unwrap_or(0).min(materials.len() - 1);
        let triangles = m.indices.chunks_exact(3).map(|idx| Triangle {
            indices: [idx[0], idx[1], idx[2]], material_id,
            paint: [255, 255, 255, 255],
        }).collect();
        if !vertices.is_empty() && !triangles.is_empty() {
            let mesh = Mesh { vertices, triangles, materials: materials.clone() };
            objects.push(Object3D::new(model.name, mesh));
        }
    }
    if objects.is_empty() { return Err(anyhow!("El OBJ no contiene triángulos")); }
    Ok(objects)
}

fn load_gltf(path: &Path) -> Result<Vec<Object3D>> {
    let (document, buffers, images) = gltf::import(path).context("No se pudo leer el archivo glTF/GLB")?;
    let embedded_textures: Vec<Option<TextureAsset>> = images.iter().map(rgba_from_gltf_image).collect();
    let mut objects = Vec::new();

    for mesh in document.meshes() {
        for (primitive_index, primitive) in mesh.primitives().enumerate() {
            if primitive.mode() != gltf::mesh::Mode::Triangles { continue; }
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
            let positions: Vec<[f32; 3]> = match reader.read_positions() {
                Some(iter) => iter.collect(),
                None => continue,
            };
            let uvs: Vec<[f32; 2]> = reader.read_tex_coords(0).map(|v| v.into_f32().collect()).unwrap_or_default();
            let vertices: Vec<Vertex> = positions.iter().enumerate().map(|(i, p)| Vertex {
                position: *p,
                uv: uvs.get(i).copied().unwrap_or([0.0, 0.0]),
            }).collect();
            let indices: Vec<u32> = reader.read_indices().map(|v| v.into_u32().collect()).unwrap_or_else(|| (0..vertices.len() as u32).collect());
            let triangles: Vec<Triangle> = indices.chunks_exact(3).map(|idx| Triangle {
                indices: [idx[0], idx[1], idx[2]], material_id: 0, paint: [255, 255, 255, 255],
            }).collect();
            if triangles.is_empty() { continue; }

            let gltf_material = primitive.material();
            let pbr = gltf_material.pbr_metallic_roughness();
            let factor = pbr.base_color_factor();
            let texture_info = pbr.base_color_texture();
            let texture_index = texture_info.map(|t| t.texture().source().index());
            let texture = texture_index.and_then(|i| embedded_textures.get(i).cloned().flatten());
            let material = Material {
                name: gltf_material.name().unwrap_or("Material glTF").to_string(),
                base_color: [
                    (factor[0].clamp(0.0, 1.0) * 255.0) as u8,
                    (factor[1].clamp(0.0, 1.0) * 255.0) as u8,
                    (factor[2].clamp(0.0, 1.0) * 255.0) as u8,
                    (factor[3].clamp(0.0, 1.0) * 255.0) as u8,
                ],
                texture,
                texture_source: texture_index.map(|i| format!("glTF image #{i}")),
            };
            let name = format!("{} / primitive {}", mesh.name().unwrap_or("Mesh"), primitive_index + 1);
            let mesh_data = Mesh { vertices, triangles, materials: vec![material] };
            objects.push(Object3D::new(name, mesh_data));
        }
    }

    if objects.is_empty() { return Err(anyhow!("El archivo no contiene primitivas triangulares compatibles")); }
    Ok(objects)
}


fn load_gltf_cameras(path: &Path) -> Result<Vec<EditorCamera>> {
    let (document, _, _) = gltf::import(path).context("No se pudieron leer las cámaras glTF")?;
    let mut cameras = Vec::new();
    for node in document.nodes() {
        let Some(camera) = node.camera() else { continue; };
        let (translation, rotation, _scale) = node.transform().decomposed();
        let q = [rotation[0], rotation[1], rotation[2]];
        // Rotate the glTF camera forward vector (local -Z) by its quaternion.
        let v = [0.0f32, 0.0, -1.0];
        let t = [
            2.0 * (q[1] * v[2] - q[2] * v[1]),
            2.0 * (q[2] * v[0] - q[0] * v[2]),
            2.0 * (q[0] * v[1] - q[1] * v[0]),
        ];
        let cross_qt = [
            q[1] * t[2] - q[2] * t[1],
            q[2] * t[0] - q[0] * t[2],
            q[0] * t[1] - q[1] * t[0],
        ];
        let forward = [
            v[0] + rotation[3] * t[0] + cross_qt[0],
            v[1] + rotation[3] * t[1] + cross_qt[1],
            v[2] + rotation[3] * t[2] + cross_qt[2],
        ];
        // The editor camera is stored as an orbit camera: eye = target + offset.
        let offset = [-forward[0], -forward[1], -forward[2]];
        let length = (offset[0]*offset[0] + offset[1]*offset[1] + offset[2]*offset[2]).sqrt().max(1e-6);
        let yaw = offset[0].atan2(offset[2]);
        let pitch = (offset[1] / length).clamp(-1.0, 1.0).asin();
        let fov_degrees = match camera.projection() {
            gltf::camera::Projection::Perspective(p) => p.yfov().to_degrees(),
            gltf::camera::Projection::Orthographic(_) => 55.0,
        };
        let distance = 3.0;
        cameras.push(EditorCamera {
            name: camera.name().unwrap_or("Camera glTF").to_string(),
            yaw,
            pitch,
            distance,
            target: [translation[0] + forward[0] * distance, translation[1] + forward[1] * distance, translation[2] + forward[2] * distance],
            fov_degrees,
        });
    }
    Ok(cameras)
}

pub fn import_into_scene(scene: &mut Scene, path: &Path) -> Result<()> {
    let mut objects = load_model(path)?;
    if scene.objects.len() == 1 && scene.objects[0].name == "Cubo de ejemplo" {
        scene.objects.clear();
    }
    scene.objects.append(&mut objects);
    scene.name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Escena").to_string();
    let mut center = [0.0; 3];
    let mut count = 0usize;
    let mut radius = 0.0f32;
    for obj in &scene.objects {
        let (c, r) = obj.mesh.bounds_center_radius();
        for axis in 0..3 { center[axis] += c[axis] + obj.position[axis]; }
        radius = radius.max(r);
        count += 1;
    }
    if count > 0 { for v in &mut center { *v /= count as f32; } }

    let mut cameras = if matches!(path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase().as_str(), "gltf" | "glb") {
        load_gltf_cameras(path).unwrap_or_default()
    } else { Vec::new() };
    if cameras.is_empty() {
        let mut camera = EditorCamera::default();
        camera.name = "Camera 1".to_string();
        camera.target = center;
        camera.distance = (radius * 3.2).max(3.0);
        cameras.push(camera);
    }
    scene.cameras = cameras;
    scene.active_camera = 0;
    Ok(())
}

