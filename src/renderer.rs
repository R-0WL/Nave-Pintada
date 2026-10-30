use nalgebra_glm as glm;

use crate::model::{EditorCamera, Object3D, Scene};

#[derive(Clone, Copy, Debug)]
pub struct ScreenPoint {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub inv_w: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PickTriangle {
    pub points: [ScreenPoint; 3],
    pub object_id: usize,
    pub triangle_id: usize,
}

pub struct Renderer {
    pub width: usize,
    pub height: usize,
    pub color: Vec<u32>,
    pub depth: Vec<f32>,
    pub picks: Vec<PickTriangle>,
}

impl Renderer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            color: vec![0x00232931; width * height],
            depth: vec![f32::INFINITY; width * height],
            picks: Vec::new(),
        }
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        if width == self.width && height == self.height { return; }
        self.width = width.max(1);
        self.height = height.max(1);
        self.color.resize(self.width * self.height, 0);
        self.depth.resize(self.width * self.height, f32::INFINITY);
    }

    pub fn render(
        &mut self,
        scene: &Scene,
        camera: &EditorCamera,
        wireframe: bool,
        show_textures: bool,
        selected: Option<(usize, usize)>,
    ) {
        self.color.fill(0x00232931);
        self.depth.fill(f32::INFINITY);
        self.picks.clear();

        let target = glm::vec3(camera.target[0], camera.target[1], camera.target[2]);
        let cp = camera.pitch.cos();
        let eye = glm::vec3(
            target.x + camera.distance * cp * camera.yaw.sin(),
            target.y + camera.distance * camera.pitch.sin(),
            target.z + camera.distance * cp * camera.yaw.cos(),
        );
        let up = glm::vec3(0.0, 1.0, 0.0);
        let view = glm::look_at(&eye, &target, &up);
        let aspect = self.width as f32 / self.height.max(1) as f32;
        let projection = glm::perspective(aspect, camera.fov_degrees.to_radians(), 0.05, 5000.0);
        let view_projection = projection * view;
        let light = normalize(scene.light_direction);

        let mut overlays: Vec<(PickTriangle, bool)> = Vec::new();
        for (object_id, object) in scene.objects.iter().enumerate() {
            if !object.visible { continue; }
            let transformed: Vec<[f32; 3]> = object.mesh.vertices.iter().map(|v| transform_vertex(v.position, object)).collect();
            let projected: Vec<Option<ScreenPoint>> = transformed.iter().map(|p| project(*p, &view_projection, self.width, self.height)).collect();

            for (triangle_id, tri) in object.mesh.triangles.iter().enumerate() {
                let Some(a) = projected.get(tri.indices[0] as usize).and_then(|v| *v) else { continue; };
                let Some(b) = projected.get(tri.indices[1] as usize).and_then(|v| *v) else { continue; };
                let Some(c) = projected.get(tri.indices[2] as usize).and_then(|v| *v) else { continue; };
                let points = [a, b, c];
                let p = [
                    transformed[tri.indices[0] as usize],
                    transformed[tri.indices[1] as usize],
                    transformed[tri.indices[2] as usize],
                ];
                let normal = normalize(cross(sub(p[1], p[0]), sub(p[2], p[0])));
                let diffuse = dot(normal, light).max(0.0);
                let illumination = 0.28 + diffuse * 0.72;
                let material = object.mesh.materials.get(tri.material_id)
                    .or_else(|| object.mesh.materials.first());
                let base = material.map(|m| m.base_color).unwrap_or([185, 190, 200, 255]);
                let texture = if show_textures { material.and_then(|m| m.texture.as_ref()) } else { None };
                let uv = [
                    object.mesh.vertices[tri.indices[0] as usize].uv,
                    object.mesh.vertices[tri.indices[1] as usize].uv,
                    object.mesh.vertices[tri.indices[2] as usize].uv,
                ];
                let paint = tri.paint;
                let pick = PickTriangle { points, object_id, triangle_id };
                self.raster_triangle(pick, uv, base, texture, paint, illumination);
                overlays.push((pick, selected == Some((object_id, triangle_id))));
            }
        }

        if wireframe {
            for (pick, is_selected) in &overlays {
                let color = if *is_selected { 0x00fff176 } else { 0x00929ba8 };
                self.draw_line(pick.points[0], pick.points[1], color);
                self.draw_line(pick.points[1], pick.points[2], color);
                self.draw_line(pick.points[2], pick.points[0], color);
            }
        }
        if let Some((_, _)) = selected {
            for (pick, is_selected) in &overlays {
                if *is_selected {
                    self.draw_line(pick.points[0], pick.points[1], 0x00ffeb3b);
                    self.draw_line(pick.points[1], pick.points[2], 0x00ffeb3b);
                    self.draw_line(pick.points[2], pick.points[0], 0x00ffeb3b);
                }
            }
        }
    }

    fn raster_triangle(
        &mut self,
        pick: PickTriangle,
        uv: [[f32; 2]; 3],
        base: [u8; 4],
        texture: Option<&crate::model::TextureAsset>,
        paint: [u8; 4],
        illumination: f32,
    ) {
        let p = pick.points;
        let min_x = p.iter().map(|v| v.x.floor() as i32).min().unwrap_or(0).max(0) as usize;
        let max_x = p.iter().map(|v| v.x.ceil() as i32).max().unwrap_or(0).min(self.width as i32 - 1).max(0) as usize;
        let min_y = p.iter().map(|v| v.y.floor() as i32).min().unwrap_or(0).max(0) as usize;
        let max_y = p.iter().map(|v| v.y.ceil() as i32).max().unwrap_or(0).min(self.height as i32 - 1).max(0) as usize;
        let area = edge(p[0].x, p[0].y, p[1].x, p[1].y, p[2].x, p[2].y);
        if area.abs() < 0.0001 { return; }

        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let w0 = edge(p[1].x, p[1].y, p[2].x, p[2].y, px, py) / area;
                let w1 = edge(p[2].x, p[2].y, p[0].x, p[0].y, px, py) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < -0.0001 || w1 < -0.0001 || w2 < -0.0001 { continue; }
                let z = w0 * p[0].z + w1 * p[1].z + w2 * p[2].z;
                if !(0.0..=1.0).contains(&z) { continue; }
                let idx = y * self.width + x;
                if z >= self.depth[idx] { continue; }
                self.depth[idx] = z;

                let denom = w0 * p[0].inv_w + w1 * p[1].inv_w + w2 * p[2].inv_w;
                let (u, v) = if denom.abs() > 1e-8 {
                    (
                        (w0 * uv[0][0] * p[0].inv_w + w1 * uv[1][0] * p[1].inv_w + w2 * uv[2][0] * p[2].inv_w) / denom,
                        (w0 * uv[0][1] * p[0].inv_w + w1 * uv[1][1] * p[1].inv_w + w2 * uv[2][1] * p[2].inv_w) / denom,
                    )
                } else { (0.0, 0.0) };

                let mut color = if paint != [255, 255, 255, 255] {
                    paint
                } else if let Some(tex) = texture {
                    let texel = tex.sample(u, v);
                    [
                        ((texel[0] as u16 * base[0] as u16) / 255) as u8,
                        ((texel[1] as u16 * base[1] as u16) / 255) as u8,
                        ((texel[2] as u16 * base[2] as u16) / 255) as u8,
                        ((texel[3] as u16 * base[3] as u16) / 255) as u8,
                    ]
                } else { base };
                color[0] = (color[0] as f32 * illumination).clamp(0.0, 255.0) as u8;
                color[1] = (color[1] as f32 * illumination).clamp(0.0, 255.0) as u8;
                color[2] = (color[2] as f32 * illumination).clamp(0.0, 255.0) as u8;
                self.color[idx] = ((color[0] as u32) << 16) | ((color[1] as u32) << 8) | color[2] as u32;
            }
        }
        self.picks.push(pick);
    }

    fn draw_line(&mut self, a: ScreenPoint, b: ScreenPoint, color: u32) {
        let mut x0 = a.x as i32;
        let mut y0 = a.y as i32;
        let x1 = b.x as i32;
        let y1 = b.y as i32;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            if x0 >= 0 && y0 >= 0 && x0 < self.width as i32 && y0 < self.height as i32 {
                self.color[y0 as usize * self.width + x0 as usize] = color;
            }
            if x0 == x1 && y0 == y1 { break; }
            let e2 = 2 * err;
            if e2 >= dy { err += dy; x0 += sx; }
            if e2 <= dx { err += dx; y0 += sy; }
        }
    }

    pub fn pick(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        let mut best: Option<(f32, usize, usize)> = None;
        for item in &self.picks {
            let p = item.points;
            let area = edge(p[0].x, p[0].y, p[1].x, p[1].y, p[2].x, p[2].y);
            if area.abs() < 0.0001 { continue; }
            let w0 = edge(p[1].x, p[1].y, p[2].x, p[2].y, x, y) / area;
            let w1 = edge(p[2].x, p[2].y, p[0].x, p[0].y, x, y) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                let z = w0 * p[0].z + w1 * p[1].z + w2 * p[2].z;
                if best.map(|b| z < b.0).unwrap_or(true) { best = Some((z, item.object_id, item.triangle_id)); }
            }
        }
        best.map(|(_, o, t)| (o, t))
    }
}

fn project(point: [f32; 3], vp: &glm::Mat4, width: usize, height: usize) -> Option<ScreenPoint> {
    let clip = vp * glm::vec4(point[0], point[1], point[2], 1.0);
    if clip.w <= 0.0001 { return None; }
    let inv_w = 1.0 / clip.w;
    let nx = clip.x * inv_w;
    let ny = clip.y * inv_w;
    let nz = clip.z * inv_w;
    if nz < -1.2 || nz > 1.2 { return None; }
    Some(ScreenPoint {
        x: (nx * 0.5 + 0.5) * width as f32,
        y: (1.0 - (ny * 0.5 + 0.5)) * height as f32,
        z: nz * 0.5 + 0.5,
        inv_w,
    })
}

fn transform_vertex(mut p: [f32; 3], object: &Object3D) -> [f32; 3] {
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

fn edge(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> f32 {
    (px - ax) * (by - ay) - (py - ay) * (bx - ax)
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] { [a[0]-b[0], a[1]-b[1], a[2]-b[2]] }
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] { [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]] }
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 { a[0]*b[0]+a[1]*b[1]+a[2]*b[2] }
fn normalize(v: [f32; 3]) -> [f32; 3] { let l = (dot(v,v)).sqrt().max(1e-8); [v[0]/l,v[1]/l,v[2]/l] }
