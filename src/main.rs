mod importer;
mod model;
mod project;
mod renderer;

use anyhow::Result;
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
use model::{EditorCamera, Scene};
use renderer::Renderer;

const WIDTH: usize = 1100;
const HEIGHT: usize = 720;

fn main() -> Result<()> {
    let mut window = Window::new(
        "Rust 3D Editor | O abrir · P pintar · W wireframe · Ctrl+S guardar · F12 captura",
        WIDTH,
        HEIGHT,
        WindowOptions { resize: false, ..WindowOptions::default() },
    )?;
    window.set_target_fps(60);

    let mut scene = Scene::default();
    let mut renderer = Renderer::new(WIDTH, HEIGHT);
    let mut selected_object: Option<usize> = Some(0);
    let mut selected_face: Option<(usize, usize)> = None;
    let mut wireframe = true;
    let mut show_textures = true;
    let mut painting = false;
    let mut camera_edit = false;
    let mut scene_edit = false;
    let mut light_edit = false;
    let mut paint_color = [235, 70, 70, 255];
    let mut previous_mouse = (0.0f32, 0.0f32);
    let mut left_was_down = false;
    let mut right_was_down = false;
    let mut middle_was_down = false;
    let mut status = "Listo. Abre un OBJ, GLTF o GLB con O.".to_string();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let camera_index = scene.active_camera.min(scene.cameras.len().saturating_sub(1));
        let mouse = window.get_mouse_pos(MouseMode::Clamp).unwrap_or(previous_mouse);
        let left_down = window.get_mouse_down(MouseButton::Left);
        let right_down = window.get_mouse_down(MouseButton::Right);
        let middle_down = window.get_mouse_down(MouseButton::Middle);

        if right_down {
            if right_was_down {
                let dx = mouse.0 - previous_mouse.0;
                let dy = mouse.1 - previous_mouse.1;
                if let Some(cam) = scene.cameras.get_mut(camera_index) {
                    cam.yaw -= dx * 0.008;
                    cam.pitch = (cam.pitch + dy * 0.008).clamp(-1.52, 1.52);
                }
            }
        }
        if middle_down {
            if middle_was_down {
                let dx = mouse.0 - previous_mouse.0;
                let dy = mouse.1 - previous_mouse.1;
                if let Some(cam) = scene.cameras.get_mut(camera_index) {
                    let right = [cam.yaw.cos(), 0.0, -cam.yaw.sin()];
                    let up = [-cam.pitch.sin() * cam.yaw.sin(), cam.pitch.cos(), -cam.pitch.sin() * cam.yaw.cos()];
                    let amount = cam.distance * 0.0018;
                    for axis in 0..3 {
                        cam.target[axis] -= right[axis] * dx * amount;
                        cam.target[axis] += up[axis] * dy * amount;
                    }
                }
            }
        }
        if let Some((_, scroll_y)) = window.get_scroll_wheel() {
            if scroll_y.abs() > 0.0 {
                if let Some(cam) = scene.cameras.get_mut(camera_index) {
                    cam.distance = (cam.distance * (1.0 - scroll_y * 0.10)).clamp(0.15, 5000.0);
                }
            }
        }

        // Click selects a triangle; in paint mode it applies the active paint color.
        if left_down && !left_was_down {
            if let Some((object_id, triangle_id)) = renderer.pick(mouse.0, mouse.1) {
                selected_object = Some(object_id);
                selected_face = Some((object_id, triangle_id));
                if painting {
                    if let Some(triangle) = scene.objects.get_mut(object_id)
                        .and_then(|o| o.mesh.triangles.get_mut(triangle_id)) {
                        triangle.paint = paint_color;
                        status = format!("Cara pintada: objeto {}, triángulo {}", object_id + 1, triangle_id + 1);
                    }
                } else {
                    status = format!("Seleccionado: objeto {}, triángulo {}", object_id + 1, triangle_id + 1);
                }
            } else {
                selected_face = None;
            }
        }

        let control = window.is_key_down(Key::LeftCtrl) || window.is_key_down(Key::RightCtrl);
        let shift = window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
        for key in window.get_keys_pressed(KeyRepeat::No) {
            match key {
                Key::O if !control => open_file(&mut scene, &mut selected_object, &mut selected_face, &mut status),
                Key::S if control => save_project(&scene, &mut status),
                Key::F12 => save_capture(&renderer, &mut status),
                Key::W => { wireframe = !wireframe; status = format!("Wireframe: {}", if wireframe { "activado" } else { "desactivado" }); }
                Key::T => { show_textures = !show_textures; status = format!("Texturas: {}", if show_textures { "activadas" } else { "desactivadas" }); }
                Key::P => { painting = !painting; status = format!("Modo pintura: {}", if painting { "ACTIVO" } else { "inactivo" }); }
                Key::C => {
                    if !scene.cameras.is_empty() {
                        scene.active_camera = (scene.active_camera + 1) % scene.cameras.len();
                        status = format!("Cámara activa: {}", scene.cameras[scene.active_camera].name);
                    }
                }
                Key::M => { camera_edit = !camera_edit; light_edit = false; scene_edit = false; status = format!("Edición de cámara: {}", if camera_edit { "ACTIVA" } else { "inactiva" }); }
                Key::G => { scene_edit = !scene_edit; camera_edit = false; light_edit = false; status = format!("Edición de escena: {}", if scene_edit { "ACTIVA" } else { "inactiva" }); }
                Key::L => { light_edit = !light_edit; camera_edit = false; scene_edit = false; status = format!("Edición de luz: {}", if light_edit { "ACTIVA" } else { "inactiva" }); }
                Key::V => {
                    let mut next = scene.cameras.get(scene.active_camera).cloned().unwrap_or_default();
                    next.name = format!("Camera {}", scene.cameras.len() + 1);
                    scene.cameras.push(next);
                    scene.active_camera = scene.cameras.len() - 1;
                    status = "Nueva cámara creada (V). Ajusta con M + flechas y mouse.".to_string();
                }
                Key::F => {
                    let idx = scene.active_camera.min(scene.cameras.len().saturating_sub(1));
                    let mut camera = scene.cameras.get(idx).cloned().unwrap_or_default();
                    frame_scene(&scene, &mut camera);
                    if let Some(slot) = scene.cameras.get_mut(idx) { *slot = camera; }
                },
                Key::Home => {
                    let idx = scene.active_camera.min(scene.cameras.len().saturating_sub(1));
                    if let Some(camera) = scene.cameras.get_mut(idx) { *camera = EditorCamera::default(); }
                    status = "Cámara restablecida".to_string();
                }
                Key::Key1 => { paint_color = [235, 70, 70, 255]; status = "Pintura: rojo".to_string(); }
                Key::Key2 => { paint_color = [80, 210, 110, 255]; status = "Pintura: verde".to_string(); }
                Key::Key3 => { paint_color = [75, 140, 245, 255]; status = "Pintura: azul".to_string(); }
                Key::Key4 => { paint_color = [245, 205, 65, 255]; status = "Pintura: amarillo".to_string(); }
                Key::Key5 => { paint_color = [255, 255, 255, 255]; status = "Pintura: blanco / material original".to_string(); }
                Key::Delete if selected_face.is_some() => {
                    if let Some((o, t)) = selected_face {
                        if let Some(face) = scene.objects.get_mut(o).and_then(|obj| obj.mesh.triangles.get_mut(t)) {
                            face.paint = [255, 255, 255, 255];
                            status = "Pintura de cara eliminada".to_string();
                        }
                    }
                }
                _ => {}
            }
        }

        // Arrow keys move the selected object. Hold Shift or enable M to move camera target.
        let step = if window.is_key_down(Key::LeftAlt) { 0.01 } else { 0.10 };
        let mut move_axis = |axis: usize, amount: f32| {
            if camera_edit || shift {
                let idx = scene.active_camera.min(scene.cameras.len().saturating_sub(1));
                if let Some(camera) = scene.cameras.get_mut(idx) { camera.target[axis] += amount; }
            } else if light_edit {
                scene.light_direction[axis] += amount;
            } else if scene_edit {
                for object in &mut scene.objects { object.position[axis] += amount; }
            } else if let Some(object) = selected_object.and_then(|i| scene.objects.get_mut(i)) {
                object.position[axis] += amount;
            }
        };
        if window.is_key_down(Key::Left) { move_axis(0, -step); }
        if window.is_key_down(Key::Right) { move_axis(0, step); }
        if window.is_key_down(Key::Up) { move_axis(1, step); }
        if window.is_key_down(Key::Down) { move_axis(1, -step); }
        if window.is_key_down(Key::PageUp) { move_axis(2, step); }
        if window.is_key_down(Key::PageDown) { move_axis(2, -step); }
        drop(move_axis);

        let active_camera = scene.cameras.get(scene.active_camera).cloned().unwrap_or_default();
        renderer.render(&scene, &active_camera, wireframe, show_textures, selected_face);
        let title = format!(
            "Rust 3D Editor | {} | {} objetos / {} triángulos | {} | {} | {}",
            scene.name,
            scene.objects.len(),
            scene.objects.iter().map(|o| o.mesh.triangles.len()).sum::<usize>(),
            if painting { "PINTURA" } else { "SELECCIÓN" },
            scene.cameras.get(scene.active_camera).map(|c| c.name.as_str()).unwrap_or("Camera"),
            status,
        );
        window.set_title(&title);
        window.update_with_buffer(&renderer.color, renderer.width, renderer.height)?;

        previous_mouse = mouse;
        left_was_down = left_down;
        right_was_down = right_down;
        middle_was_down = middle_down;
    }
    Ok(())
}

fn frame_scene(scene: &Scene, camera: &mut EditorCamera) {
    if scene.objects.is_empty() { return; }
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for object in &scene.objects {
        let (center, radius) = object.mesh.bounds_center_radius();
        for axis in 0..3 {
            let c = center[axis] + object.position[axis];
            min[axis] = min[axis].min(c - radius);
            max[axis] = max[axis].max(c + radius);
        }
    }
    for axis in 0..3 { camera.target[axis] = (min[axis] + max[axis]) * 0.5; }
    let dx = max[0] - min[0]; let dy = max[1] - min[1]; let dz = max[2] - min[2];
    camera.distance = (dx.max(dy).max(dz) * 1.8).max(2.0);
}

fn open_file(scene: &mut Scene, selected: &mut Option<usize>, face: &mut Option<(usize, usize)>, status: &mut String) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Modelos 3D", &["obj", "gltf", "glb"])
        .add_filter("Proyecto Rust 3D", &["r3d", "json"])
        .pick_file() else { return; };
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    if extension == "r3d" || extension == "json" {
        match project::load_scene(&path) {
            Ok(loaded) => {
                *scene = loaded;
                *selected = if scene.objects.is_empty() { None } else { Some(0) };
                *face = None;
                *status = format!("Proyecto abierto: {}", path.display());
            }
            Err(error) => *status = format!("Error al abrir proyecto: {error:#}"),
        }
    } else {
        match importer::import_into_scene(scene, &path) {
            Ok(()) => {
                *selected = if scene.objects.is_empty() { None } else { Some(0) };
                *face = None;
                *status = format!("Modelo importado: {}", path.display());
            }
            Err(error) => *status = format!("Error al importar: {error:#}"),
        }
    }
}

fn save_project(scene: &Scene, status: &mut String) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Proyecto Rust 3D", &["r3d"])
        .set_file_name("proyecto.r3d")
        .save_file() else { return; };
    match project::save_scene(scene, &path) {
        Ok(()) => *status = format!("Proyecto guardado: {}", path.display()),
        Err(error) => *status = format!("Error al guardar: {error:#}"),
    }
}

fn save_capture(renderer: &Renderer, status: &mut String) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Imagen PNG", &["png"])
        .set_file_name("captura.png")
        .save_file() else { return; };
    match project::save_screenshot(&renderer.color, renderer.width, renderer.height, &path) {
        Ok(()) => *status = format!("Captura guardada: {}", path.display()),
        Err(error) => *status = format!("Error al capturar: {error:#}"),
    }
}
