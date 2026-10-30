use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Triangle {
    pub indices: [u32; 3],
    pub material_id: usize,
    /// Per-face paint overlay. White means "use the material color/texture".
    pub paint: [u8; 4],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextureAsset {
    pub width: u32,
    pub height: u32,
    /// Packed RGBA8 pixels. Kept in the project so embedded GLB textures survive saving.
    pub pixels: Vec<u8>,
}

impl TextureAsset {
    pub fn sample(&self, u: f32, v: f32) -> [u8; 4] {
        if self.width == 0 || self.height == 0 || self.pixels.len() < 4 {
            return [255, 255, 255, 255];
        }
        // Repeat UVs and flip V (image coordinates start at the top).
        let u = u.rem_euclid(1.0);
        let v = (1.0 - v).rem_euclid(1.0);
        let x = ((u * self.width as f32) as u32).min(self.width - 1);
        let y = ((v * self.height as f32) as u32).min(self.height - 1);
        let i = ((y * self.width + x) * 4) as usize;
        if i + 3 >= self.pixels.len() { return [255, 255, 255, 255]; }
        [self.pixels[i], self.pixels[i + 1], self.pixels[i + 2], self.pixels[i + 3]]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Material {
    pub name: String,
    pub base_color: [u8; 4],
    pub texture: Option<TextureAsset>,
    pub texture_source: Option<String>,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            name: "Material".to_string(),
            base_color: [190, 195, 205, 255],
            texture: None,
            texture_source: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub triangles: Vec<Triangle>,
    pub materials: Vec<Material>,
}

impl Mesh {
    pub fn bounds_center_radius(&self) -> ([f32; 3], f32) {
        if self.vertices.is_empty() {
            return ([0.0, 0.0, 0.0], 1.0);
        }
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for v in &self.vertices {
            for axis in 0..3 {
                min[axis] = min[axis].min(v.position[axis]);
                max[axis] = max[axis].max(v.position[axis]);
            }
        }
        let center = [
            (min[0] + max[0]) * 0.5,
            (min[1] + max[1]) * 0.5,
            (min[2] + max[2]) * 0.5,
        ];
        let mut radius = 0.0f32;
        for v in &self.vertices {
            let dx = v.position[0] - center[0];
            let dy = v.position[1] - center[1];
            let dz = v.position[2] - center[2];
            radius = radius.max((dx * dx + dy * dy + dz * dz).sqrt());
        }
        (center, radius.max(0.01))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Object3D {
    pub name: String,
    pub mesh: Mesh,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    pub visible: bool,
}

impl Object3D {
    pub fn new(name: String, mesh: Mesh) -> Self {
        Self {
            name,
            mesh,
            position: [0.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0],
            scale: [1.0, 1.0, 1.0],
            visible: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditorCamera {
    pub name: String,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: [f32; 3],
    pub fov_degrees: f32,
}

impl Default for EditorCamera {
    fn default() -> Self {
        Self {
            name: "Camera 1".to_string(),
            yaw: 0.55,
            pitch: 0.25,
            distance: 5.0,
            target: [0.0, 0.0, 0.0],
            fov_degrees: 55.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scene {
    pub name: String,
    pub objects: Vec<Object3D>,
    pub cameras: Vec<EditorCamera>,
    pub active_camera: usize,
    pub light_direction: [f32; 3],
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            name: "Escena sin título".to_string(),
            objects: vec![Object3D::new("Cubo de ejemplo".to_string(), demo_cube())],
            cameras: vec![EditorCamera::default()],
            active_camera: 0,
            light_direction: [-0.4, 0.8, 0.5],
        }
    }
}

pub fn demo_cube() -> Mesh {
    let p = [
        [-1.0, -1.0, -1.0], [1.0, -1.0, -1.0], [1.0, 1.0, -1.0], [-1.0, 1.0, -1.0],
        [-1.0, -1.0, 1.0], [1.0, -1.0, 1.0], [1.0, 1.0, 1.0], [-1.0, 1.0, 1.0],
    ];
    let vertices = p.iter().map(|position| Vertex { position: *position, uv: [0.0, 0.0] }).collect();
    let faces: [[u32; 3]; 12] = [
        [0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7],
        [0, 1, 5], [0, 5, 4], [3, 7, 6], [3, 6, 2],
        [0, 4, 7], [0, 7, 3], [1, 2, 6], [1, 6, 5],
    ];
    let triangles = faces.iter().map(|indices| Triangle { indices: *indices, material_id: 0, paint: [255, 255, 255, 255] }).collect();
    Mesh { vertices, triangles, materials: vec![Material::default()] }
}
