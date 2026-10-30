use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::model::{Material, Object3D, Scene, TextureAsset};
use crate::project;

/// Creates a complete, self-contained export folder containing the editable
/// project, OBJ/MTL, glTF, GLB and a viewport capture.
pub fn export_bundle(scene: &Scene, framebuffer: &[u32], width: usize, height: usize) -> Result<PathBuf> {
    let root = std::env::current_dir().context("No se pudo obtener la carpeta de trabajo")?.join("exports");
    fs::create_dir_all(&root).with_context(|| format!("No se pudo crear {}", root.display()))?;

    let millis = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    let base = format!("export_{}_{}", millis, safe_name(&scene.name));
    let mut export_dir = root.join(&base);
    let mut suffix = 1usize;
    while export_dir.exists() {
        export_dir = root.join(format!("{}_{}", base, suffix));
        suffix += 1;
    }

    let native_dir = export_dir.join("native");
    let obj_dir = export_dir.join("obj");
    let gltf_dir = export_dir.join("gltf");
    let glb_dir = export_dir.join("glb");
    let capture_dir = export_dir.join("captures");
    for dir in [&native_dir, &obj_dir, &gltf_dir, &glb_dir, &capture_dir] {
        fs::create_dir_all(dir).with_context(|| format!("No se pudo crear {}", dir.display()))?;
    }

    project::save_scene(scene, &native_dir.join("scene.r3d"))?;
    write_obj(scene, &obj_dir)?;
    export_gltf(scene, &gltf_dir, false)?;
    export_gltf(scene, &glb_dir, true)?;
    project::save_screenshot(framebuffer, width, height, &capture_dir.join("viewport.png"))?;

    let triangle_count: usize = scene.objects.iter().map(|o| o.mesh.triangles.len()).sum();
    let manifest = json!({
        "application": "Rust 3D Editor",
        "scene": scene.name,
        "objects": scene.objects.len(),
        "triangles": triangle_count,
        "formats": ["R3D", "OBJ/MTL", "glTF 2.0", "GLB 2.0", "PNG"],
        "folders": {
            "native": "native/scene.r3d",
            "obj": "obj/scene.obj + scene.mtl + textures/",
            "gltf": "gltf/scene.gltf + scene.bin + textures/",
            "glb": "glb/scene.glb (geometría y texturas embebidas)",
            "capture": "captures/viewport.png"
        }
    });
    fs::write(export_dir.join("manifest.json"), serde_json::to_vec_pretty(&manifest)?)
        .context("No se pudo escribir manifest.json")?;

    Ok(export_dir)
}

pub fn save_capture_in_export(export_dir: &Path, framebuffer: &[u32], width: usize, height: usize) -> Result<PathBuf> {
    let captures = export_dir.join("captures");
    fs::create_dir_all(&captures).with_context(|| format!("No se pudo crear {}", captures.display()))?;
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    let path = captures.join(format!("capture_{millis}.png"));
    project::save_screenshot(framebuffer, width, height, &path)?;
    Ok(path)
}

fn safe_name(value: &str) -> String {
    let value: String = value.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let trimmed = value.trim_matches('_');
    if trimmed.is_empty() { "scene".to_string() } else { trimmed.chars().take(48).collect() }
}

fn transform_position(mut p: [f32; 3], object: &Object3D) -> [f32; 3] {
    for axis in 0..3 { p[axis] *= object.scale[axis]; }
    let (sx, cx) = object.rotation[0].sin_cos();
    let (sy, cy) = object.rotation[1].sin_cos();
    let (sz, cz) = object.rotation[2].sin_cos();
    let y = p[1] * cx - p[2] * sx;
    let z = p[1] * sx + p[2] * cx;
    p[1] = y; p[2] = z;
    let x = p[0] * cy + p[2] * sy;
    let z = -p[0] * sy + p[2] * cy;
    p[0] = x; p[2] = z;
    let x = p[0] * cz - p[1] * sz;
    let y = p[0] * sz + p[1] * cz;
    p[0] = x; p[1] = y;
    for axis in 0..3 { p[axis] += object.position[axis]; }
    p
}

fn texture_to_png(texture: &TextureAsset, path: &Path) -> Result<()> {
    let expected = texture.width as usize * texture.height as usize * 4;
    if texture.width == 0 || texture.height == 0 || texture.pixels.len() < expected {
        return Err(anyhow!("Textura inválida: {}x{} ({} bytes)", texture.width, texture.height, texture.pixels.len()));
    }
    let image = image::RgbaImage::from_raw(texture.width, texture.height, texture.pixels[..expected].to_vec())
        .ok_or_else(|| anyhow!("No se pudo construir la imagen de textura"))?;
    image.save(path).with_context(|| format!("No se pudo guardar textura {}", path.display()))?;
    Ok(())
}

fn write_obj(scene: &Scene, dir: &Path) -> Result<()> {
    fs::create_dir_all(dir.join("textures"))?;
    let mut obj = String::from("# Exportado por Rust 3D Editor\nmtllib scene.mtl\n");
    let mut mtl = String::from("# Materiales exportados por Rust 3D Editor\n");
    let mut vertex_offset = 1usize;
    let mut uv_offset = 1usize;

    for (object_id, object) in scene.objects.iter().enumerate() {
        if !object.visible { continue; }
        obj.push_str(&format!("\no {}\ng {}\n", safe_name(&object.name), safe_name(&object.name)));
        for vertex in &object.mesh.vertices {
            let p = transform_position(vertex.position, object);
            obj.push_str(&format!("v {:.8} {:.8} {:.8}\n", p[0], p[1], p[2]));
        }
        for vertex in &object.mesh.vertices {
            obj.push_str(&format!("vt {:.8} {:.8}\n", vertex.uv[0], vertex.uv[1]));
        }

        let material_count = object.mesh.materials.len().max(1);
        let mut material_names = Vec::with_capacity(material_count);
        for material_id in 0..material_count {
            let material = object.mesh.materials.get(material_id).cloned().unwrap_or_default();
            let name = format!("object_{object_id}_material_{material_id}");
            material_names.push(name.clone());
            mtl.push_str(&format!("\nnewmtl {name}\nKd {:.6} {:.6} {:.6}\nKa 0.0 0.0 0.0\n", material.base_color[0] as f32 / 255.0, material.base_color[1] as f32 / 255.0, material.base_color[2] as f32 / 255.0));
            if let Some(texture) = material.texture.as_ref() {
                let file_name = format!("object_{object_id}_material_{material_id}.png");
                texture_to_png(texture, &dir.join("textures").join(&file_name))?;
                mtl.push_str(&format!("map_Kd textures/{file_name}\n"));
            }
        }

        let mut paint_names = BTreeMap::<[u8; 4], String>::new();
        for triangle in &object.mesh.triangles {
            if triangle.paint != [255, 255, 255, 255] && !paint_names.contains_key(&triangle.paint) {
                let color = triangle.paint;
                let name = format!("object_{object_id}_paint_{:02x}{:02x}{:02x}", color[0], color[1], color[2]);
                paint_names.insert(color, name.clone());
                mtl.push_str(&format!("\nnewmtl {name}\nKd {:.6} {:.6} {:.6}\nKa 0.0 0.0 0.0\n", color[0] as f32 / 255.0, color[1] as f32 / 255.0, color[2] as f32 / 255.0));
            }
        }

        let mut last_material = String::new();
        for triangle in &object.mesh.triangles {
            if triangle.indices.iter().any(|&i| i as usize >= object.mesh.vertices.len()) { continue; }
            let material_name = if triangle.paint != [255, 255, 255, 255] {
                paint_names.get(&triangle.paint).cloned().unwrap_or_else(|| material_names[0].clone())
            } else {
                material_names.get(triangle.material_id.min(material_names.len() - 1)).cloned().unwrap_or_else(|| material_names[0].clone())
            };
            if material_name != last_material {
                obj.push_str(&format!("usemtl {material_name}\n"));
                last_material = material_name;
            }
            let a = triangle.indices[0] as usize + vertex_offset;
            let b = triangle.indices[1] as usize + vertex_offset;
            let c = triangle.indices[2] as usize + vertex_offset;
            let ua = triangle.indices[0] as usize + uv_offset;
            let ub = triangle.indices[1] as usize + uv_offset;
            let uc = triangle.indices[2] as usize + uv_offset;
            obj.push_str(&format!("f {a}/{ua} {b}/{ub} {c}/{uc}\n"));
        }
        vertex_offset += object.mesh.vertices.len();
        uv_offset += object.mesh.vertices.len();
    }

    fs::write(dir.join("scene.obj"), obj).context("No se pudo escribir scene.obj")?;
    fs::write(dir.join("scene.mtl"), mtl).context("No se pudo escribir scene.mtl")?;
    Ok(())
}

fn append_aligned(bin: &mut Vec<u8>) {
    while bin.len() % 4 != 0 { bin.push(0); }
}

fn append_f32(bin: &mut Vec<u8>, values: impl IntoIterator<Item = f32>) {
    append_aligned(bin);
    for value in values { bin.extend_from_slice(&value.to_le_bytes()); }
}

fn append_u32(bin: &mut Vec<u8>, values: &[u32]) -> usize {
    append_aligned(bin);
    let offset = bin.len();
    for value in values { bin.extend_from_slice(&value.to_le_bytes()); }
    offset
}

fn add_view(views: &mut Vec<Value>, offset: usize, length: usize, target: Option<u32>) -> usize {
    let mut value = json!({ "buffer": 0, "byteOffset": offset, "byteLength": length });
    if let Some(target) = target { value["target"] = json!(target); }
    views.push(value);
    views.len() - 1
}

fn add_accessor(accessors: &mut Vec<Value>, view: usize, component_type: u32, count: usize, kind: &str, min: Option<Value>, max: Option<Value>) -> usize {
    let mut value = json!({ "bufferView": view, "componentType": component_type, "count": count, "type": kind });
    if let Some(min) = min { value["min"] = min; }
    if let Some(max) = max { value["max"] = max; }
    accessors.push(value);
    accessors.len() - 1
}

fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0]-a[0], b[1]-a[1], b[2]-a[2]];
    let v = [c[0]-a[0], c[1]-a[1], c[2]-a[2]];
    let n = [u[1]*v[2]-u[2]*v[1], u[2]*v[0]-u[0]*v[2], u[0]*v[1]-u[1]*v[0]];
    let len = (n[0]*n[0] + n[1]*n[1] + n[2]*n[2]).sqrt().max(1e-12);
    [n[0]/len, n[1]/len, n[2]/len]
}

fn export_gltf(scene: &Scene, dir: &Path, as_glb: bool) -> Result<()> {
    if !as_glb { fs::create_dir_all(dir.join("textures"))?; }
    let mut bin = Vec::<u8>::new();
    let mut buffer_views = Vec::<Value>::new();
    let mut accessors = Vec::<Value>::new();
    let mut materials_json = Vec::<Value>::new();
    let mut images_json = Vec::<Value>::new();
    let mut textures_json = Vec::<Value>::new();
    let mut meshes_json = Vec::<Value>::new();
    let mut nodes_json = Vec::<Value>::new();

    for (object_id, object) in scene.objects.iter().enumerate() {
        if !object.visible || object.mesh.vertices.is_empty() || object.mesh.triangles.is_empty() { continue; }

        let mut local_materials = Vec::<usize>::new();
        let material_count = object.mesh.materials.len().max(1);
        for material_id in 0..material_count {
            let material = object.mesh.materials.get(material_id).cloned().unwrap_or_default();
            let mut material_json = make_gltf_material(&material);
            if let Some(texture) = material.texture.as_ref() {
                let png = texture_png_bytes(texture)?;
                let image_index = images_json.len();
                if as_glb {
                    append_aligned(&mut bin);
                    let offset = bin.len();
                    bin.extend_from_slice(&png);
                    let view_index = add_view(&mut buffer_views, offset, png.len(), None);
                    images_json.push(json!({ "bufferView": view_index, "mimeType": "image/png" }));
                } else {
                    let file_name = format!("object_{object_id}_material_{material_id}.png");
                    fs::write(dir.join("textures").join(&file_name), &png)
                        .with_context(|| format!("No se pudo escribir textura {file_name}"))?;
                    images_json.push(json!({ "uri": format!("textures/{file_name}") }));
                }
                let texture_index = textures_json.len();
                textures_json.push(json!({ "sampler": 0, "source": image_index }));
                material_json["pbrMetallicRoughness"]["baseColorTexture"] = json!({ "index": texture_index });
            }
            materials_json.push(material_json);
            local_materials.push(materials_json.len() - 1);
        }

        let mut paint_materials = BTreeMap::<[u8; 4], usize>::new();
        for triangle in &object.mesh.triangles {
            if triangle.paint != [255, 255, 255, 255] && !paint_materials.contains_key(&triangle.paint) {
                let material = Material {
                    name: format!("Paint_{:02X}{:02X}{:02X}", triangle.paint[0], triangle.paint[1], triangle.paint[2]),
                    base_color: triangle.paint,
                    texture: None,
                    texture_source: None,
                };
                materials_json.push(make_gltf_material(&material));
                paint_materials.insert(triangle.paint, materials_json.len() - 1);
            }
        }

        let positions: Vec<[f32; 3]> = object.mesh.vertices.iter().map(|v| transform_position(v.position, object)).collect();
        let mut normals = vec![[0.0f32; 3]; positions.len()];
        let mut groups = BTreeMap::<usize, Vec<u32>>::new();
        for triangle in &object.mesh.triangles {
            let ids = triangle.indices;
            if ids.iter().any(|&i| i as usize >= positions.len()) { continue; }
            let n = face_normal(positions[ids[0] as usize], positions[ids[1] as usize], positions[ids[2] as usize]);
            for id in ids { for axis in 0..3 { normals[id as usize][axis] += n[axis]; } }
            let material_index = if triangle.paint != [255, 255, 255, 255] {
                *paint_materials.get(&triangle.paint).unwrap_or(&local_materials[0])
            } else {
                local_materials[triangle.material_id.min(local_materials.len() - 1)]
            };
            groups.entry(material_index).or_default().extend_from_slice(&ids);
        }
        for normal in &mut normals {
            let len = (normal[0]*normal[0] + normal[1]*normal[1] + normal[2]*normal[2]).sqrt().max(1e-12);
            for axis in 0..3 { normal[axis] /= len; }
        }

        let position_values = positions.iter().flat_map(|v| v.iter().copied()).collect::<Vec<_>>();
        let normal_values = normals.iter().flat_map(|v| v.iter().copied()).collect::<Vec<_>>();
        let uv_values = object.mesh.vertices.iter().flat_map(|v| v.uv).collect::<Vec<_>>();
        let min = [
            positions.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min),
            positions.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min),
            positions.iter().map(|p| p[2]).fold(f32::INFINITY, f32::min),
        ];
        let max = [
            positions.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max),
            positions.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max),
            positions.iter().map(|p| p[2]).fold(f32::NEG_INFINITY, f32::max),
        ];
        append_f32(&mut bin, position_values);
        let pos_offset = bin.len() - positions.len() * 12;
        let pos_view = add_view(&mut buffer_views, pos_offset, positions.len() * 12, Some(34962));
        let pos_accessor = add_accessor(&mut accessors, pos_view, 5126, positions.len(), "VEC3", Some(json!(min)), Some(json!(max)));

        append_f32(&mut bin, normal_values);
        let norm_offset = bin.len() - normals.len() * 12;
        let norm_view = add_view(&mut buffer_views, norm_offset, normals.len() * 12, Some(34962));
        let norm_accessor = add_accessor(&mut accessors, norm_view, 5126, normals.len(), "VEC3", None, None);

        append_f32(&mut bin, uv_values);
        let uv_offset = bin.len() - object.mesh.vertices.len() * 8;
        let uv_view = add_view(&mut buffer_views, uv_offset, object.mesh.vertices.len() * 8, Some(34962));
        let uv_accessor = add_accessor(&mut accessors, uv_view, 5126, object.mesh.vertices.len(), "VEC2", None, None);

        let mut primitives = Vec::<Value>::new();
        for (material_index, indices) in groups {
            if indices.is_empty() { continue; }
            let offset = append_u32(&mut bin, &indices);
            let view = add_view(&mut buffer_views, offset, indices.len() * 4, Some(34963));
            let accessor = add_accessor(&mut accessors, view, 5125, indices.len(), "SCALAR", None, None);
            primitives.push(json!({
                "attributes": { "POSITION": pos_accessor, "NORMAL": norm_accessor, "TEXCOORD_0": uv_accessor },
                "indices": accessor,
                "material": material_index,
                "mode": 4
            }));
        }
        if primitives.is_empty() { continue; }
        let mesh_index = meshes_json.len();
        meshes_json.push(json!({ "name": object.name, "primitives": primitives }));
        nodes_json.push(json!({ "name": object.name, "mesh": mesh_index }));
    }

    let mut root = json!({
        "asset": { "version": "2.0", "generator": "Rust 3D Editor" },
        "scene": 0,
        "scenes": [{ "name": scene.name, "nodes": (0..nodes_json.len()).collect::<Vec<usize>>() }],
        "nodes": nodes_json,
        "meshes": meshes_json,
        "materials": materials_json,
        "accessors": accessors,
        "bufferViews": buffer_views,
        "buffers": [{ "byteLength": bin.len() }]
    });
    if !images_json.is_empty() {
        root["images"] = json!(images_json);
        root["textures"] = json!(textures_json);
        root["samplers"] = json!([{ "magFilter": 9729, "minFilter": 9987, "wrapS": 10497, "wrapT": 10497 }]);
    }

    if as_glb {
        write_glb(&dir.join("scene.glb"), root, bin)?;
    } else {
        root["buffers"][0]["uri"] = json!("scene.bin");
        fs::write(dir.join("scene.bin"), &bin).context("No se pudo escribir scene.bin")?;
        fs::write(dir.join("scene.gltf"), serde_json::to_vec_pretty(&root)?)
            .context("No se pudo escribir scene.gltf")?;
    }
    Ok(())
}

fn make_gltf_material(material: &Material) -> Value {
    json!({
        "name": material.name,
        "pbrMetallicRoughness": {
            "baseColorFactor": [
                material.base_color[0] as f32 / 255.0,
                material.base_color[1] as f32 / 255.0,
                material.base_color[2] as f32 / 255.0,
                material.base_color[3] as f32 / 255.0
            ],
            "metallicFactor": 0.0,
            "roughnessFactor": 1.0
        }
    })
}

fn texture_png_bytes(texture: &TextureAsset) -> Result<Vec<u8>> {
    let expected = texture.width as usize * texture.height as usize * 4;
    if texture.width == 0 || texture.height == 0 || texture.pixels.len() < expected {
        return Err(anyhow!("Textura inválida: {}x{}", texture.width, texture.height));
    }
    let image = image::RgbaImage::from_raw(texture.width, texture.height, texture.pixels[..expected].to_vec())
        .ok_or_else(|| anyhow!("No se pudo construir PNG de textura"))?;
    let mut cursor = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image).write_to(&mut cursor, image::ImageFormat::Png)
        .context("No se pudo codificar textura PNG")?;
    Ok(cursor.into_inner())
}

fn write_glb(path: &Path, mut json_doc: Value, mut bin: Vec<u8>) -> Result<()> {
    append_aligned(&mut bin);
    json_doc["buffers"][0].as_object_mut().ok_or_else(|| anyhow!("Documento glTF sin buffers"))?
        .insert("byteLength".to_string(), json!(bin.len()));
    let mut json_bytes = serde_json::to_vec(&json_doc)?;
    while json_bytes.len() % 4 != 0 { json_bytes.push(b' '); }
    while bin.len() % 4 != 0 { bin.push(0); }

    let total_length = 12usize + 8 + json_bytes.len() + 8 + bin.len();
    let mut glb = Vec::with_capacity(total_length);
    glb.extend_from_slice(&0x4654_6C67u32.to_le_bytes()); // glTF
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&(total_length as u32).to_le_bytes());
    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x4E4F_534Au32.to_le_bytes()); // JSON
    glb.extend_from_slice(&json_bytes);
    glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x004E_4942u32.to_le_bytes()); // BIN
    glb.extend_from_slice(&bin);
    fs::write(path, glb).with_context(|| format!("No se pudo escribir {}", path.display()))?;
    Ok(())
}
