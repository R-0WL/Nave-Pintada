mod export;
mod importer;
mod model;
mod project;
mod renderer;

use anyhow::Result;
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
use model::{EditorCamera, Scene};
use renderer::Renderer;
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::io::{self, Write};
#[cfg(target_os = "linux")]
use std::process::Command;

const WIDTH: usize = 1100;
const HEIGHT: usize = 720;
const TOOLBAR_HEIGHT: f32 = 34.0;

#[derive(Clone, Copy, Debug)]
enum ToolbarAction { Open, Demo, Export, Wire, Texture, Paint }

fn main() -> Result<()> {
    let mut window = Window::new(
        "Rust 3D Editor | OPEN/F2 · DEMO/D · EXPORT/F3 · PAINT/P · TEXT/T",
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
    let mut status = "Listo. OPEN para importar · DEMO para cargar el modelo texturizado.".to_string();
    let mut last_export: Option<PathBuf> = None;

    // Start with the included OBJ + MTL + checker.png so the texture path is
    // demonstrated immediately instead of showing an untextured placeholder cube.
    load_demo(&mut scene, &mut selected_object, &mut selected_face, &mut status);

    // Optional command-line model: cargo run --release -- path/to/model.obj
    if let Some(argument) = std::env::args_os().nth(1) {
        let path = PathBuf::from(argument);
        if path.is_file() {
            open_path(&mut scene, &path, &mut selected_object, &mut selected_face, &mut status);
        } else {
            status = format!("No existe el archivo indicado: {}", path.display());
        }
    }

    let mut display_buffer = vec![0u32; WIDTH * HEIGHT];

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let camera_index = scene.active_camera.min(scene.cameras.len().saturating_sub(1));
        let mouse = window.get_mouse_pos(MouseMode::Clamp).unwrap_or(previous_mouse);
        let left_down = window.get_mouse_down(MouseButton::Left);
        let right_down = window.get_mouse_down(MouseButton::Right);
        let middle_down = window.get_mouse_down(MouseButton::Middle);
        let left_pressed = left_down && !left_was_down;

        // Clickable toolbar makes file open/export discoverable even if the
        // keyboard shortcut is unfamiliar or the title text is overlooked.
        let toolbar_action = if left_pressed && mouse.1 < TOOLBAR_HEIGHT {
            toolbar_action_at(mouse.0)
        } else { None };
        match toolbar_action {
            Some(ToolbarAction::Open) => { last_export = None; open_file(&mut scene, &mut selected_object, &mut selected_face, &mut status); }
            Some(ToolbarAction::Demo) => { last_export = None; load_demo(&mut scene, &mut selected_object, &mut selected_face, &mut status); }
            Some(ToolbarAction::Export) => export_all(&scene, &renderer, &mut last_export, &mut status),
            Some(ToolbarAction::Wire) => { wireframe = !wireframe; status = format!("Wireframe: {}", on_off(wireframe)); }
            Some(ToolbarAction::Texture) => { show_textures = !show_textures; status = format!("Texturas: {}", on_off(show_textures)); }
            Some(ToolbarAction::Paint) => { painting = !painting; status = format!("Modo pintura: {}", on_off(painting)); }
            None => {}
        }

        if right_down && right_was_down {
            let dx = mouse.0 - previous_mouse.0;
            let dy = mouse.1 - previous_mouse.1;
            if let Some(cam) = scene.cameras.get_mut(camera_index) {
                cam.yaw -= dx * 0.008;
                cam.pitch = (cam.pitch + dy * 0.008).clamp(-1.52, 1.52);
            }
        }
        if middle_down && middle_was_down {
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
        if let Some((_, scroll_y)) = window.get_scroll_wheel() {
            if scroll_y.abs() > 0.0 {
                if let Some(cam) = scene.cameras.get_mut(camera_index) {
                    cam.distance = (cam.distance * (1.0 - scroll_y * 0.10)).clamp(0.15, 5000.0);
                }
            }
        }

        // Don't accidentally select a triangle when the toolbar was clicked.
        if left_pressed && mouse.1 >= TOOLBAR_HEIGHT {
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
                Key::O | Key::F2 if !control => { last_export = None; open_file(&mut scene, &mut selected_object, &mut selected_face, &mut status); }
                Key::D if !control => { last_export = None; load_demo(&mut scene, &mut selected_object, &mut selected_face, &mut status); }
                Key::S if control => export_all(&scene, &renderer, &mut last_export, &mut status),
                Key::F3 => export_all(&scene, &renderer, &mut last_export, &mut status),
                Key::F12 => save_capture(&scene, &renderer, &mut last_export, &mut status),
                Key::W => { wireframe = !wireframe; status = format!("Wireframe: {}", on_off(wireframe)); }
                Key::T => { show_textures = !show_textures; status = format!("Texturas: {}", on_off(show_textures)); }
                Key::P => { painting = !painting; status = format!("Modo pintura: {}", on_off(painting)); }
                Key::C => {
                    if !scene.cameras.is_empty() {
                        scene.active_camera = (scene.active_camera + 1) % scene.cameras.len();
                        status = format!("Cámara activa: {}", scene.cameras[scene.active_camera].name);
                    }
                }
                Key::M => { camera_edit = !camera_edit; light_edit = false; scene_edit = false; status = format!("Edición de cámara: {}", on_off(camera_edit)); }
                Key::G => { scene_edit = !scene_edit; camera_edit = false; light_edit = false; status = format!("Edición de escena: {}", on_off(scene_edit)); }
                Key::L => { light_edit = !light_edit; camera_edit = false; scene_edit = false; status = format!("Edición de luz: {}", on_off(light_edit)); }
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
                    status = "Escena encuadrada".to_string();
                }
                Key::Home => {
                    let idx = scene.active_camera.min(scene.cameras.len().saturating_sub(1));
                    if let Some(camera) = scene.cameras.get_mut(idx) { *camera = EditorCamera::default(); }
                    status = "Cámara restablecida".to_string();
                }
                Key::Key1 => { paint_color = [235, 70, 70, 255]; status = "Pintura: rojo".to_string(); }
                Key::Key2 => { paint_color = [80, 210, 110, 255]; status = "Pintura: verde".to_string(); }
                Key::Key3 => { paint_color = [75, 140, 245, 255]; status = "Pintura: azul".to_string(); }
                Key::Key4 => { paint_color = [245, 205, 65, 255]; status = "Pintura: amarillo".to_string(); }
                Key::Key5 => { paint_color = [255, 255, 255, 255]; status = "Pintura: material original".to_string(); }
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

        // Arrow keys move the selected object. Hold Shift or M to move camera target.
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
        display_buffer.copy_from_slice(&renderer.color);
        draw_toolbar(&mut display_buffer, WIDTH, HEIGHT, wireframe, show_textures, painting);

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
        window.update_with_buffer(&display_buffer, renderer.width, renderer.height)?;

        previous_mouse = mouse;
        left_was_down = left_down;
        right_was_down = right_down;
        middle_was_down = middle_down;
    }
    Ok(())
}

fn on_off(value: bool) -> &'static str { if value { "ACTIVO" } else { "inactivo" } }

fn toolbar_action_at(x: f32) -> Option<ToolbarAction> {
    let x = x as usize;
    match x {
        8..=70 => Some(ToolbarAction::Open),
        76..=136 => Some(ToolbarAction::Demo),
        142..=220 => Some(ToolbarAction::Export),
        226..=286 => Some(ToolbarAction::Wire),
        292..=354 => Some(ToolbarAction::Texture),
        360..=426 => Some(ToolbarAction::Paint),
        _ => None,
    }
}

fn draw_toolbar(buffer: &mut [u32], width: usize, height: usize, wire: bool, textures: bool, painting: bool) {
    let bar_h = TOOLBAR_HEIGHT as usize;
    for y in 0..bar_h.min(height) {
        for x in 0..width { buffer[y * width + x] = 0x00171c23; }
    }
    draw_button(buffer, width, 8, 5, 62, 24, "OPEN", 0x003b82c4);
    draw_button(buffer, width, 76, 5, 60, 24, "DEMO", 0x004c657c);
    draw_button(buffer, width, 142, 5, 78, 24, "EXPORT", 0x002d8b70);
    draw_button(buffer, width, 226, 5, 60, 24, "WIRE", if wire { 0x005a6470 } else { 0x00363d46 });
    draw_button(buffer, width, 292, 5, 62, 24, "TEXT", if textures { 0x005a6470 } else { 0x00363d46 });
    draw_button(buffer, width, 360, 5, 66, 24, "PAINT", if painting { 0x00904d58 } else { 0x00363d46 });
}

fn draw_button(buffer: &mut [u32], width: usize, x: usize, y: usize, w: usize, h: usize, label: &str, color: u32) {
    let height = buffer.len() / width;
    for py in y..(y + h).min(height) {
        for px in x..(x + w).min(width) { buffer[py * width + px] = color; }
    }
    draw_text(buffer, width, x + 7, y + 9, label, 0x00ffffff);
}

// Tiny 5x7 bitmap font, avoiding a full GUI dependency for the minimal toolbar.
fn draw_text(buffer: &mut [u32], width: usize, x: usize, y: usize, text: &str, color: u32) {
    let height = buffer.len() / width;
    let mut cursor_x = x;
    for ch in text.chars() {
        let rows = glyph(ch);
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..5 {
                if bits & (1 << (4 - col)) != 0 {
                    let px = cursor_x + col;
                    let py = y + row;
                    if px < width && py < height { buffer[py * width + px] = color; }
                }
            }
        }
        cursor_x += 6;
    }
}

fn glyph(ch: char) -> [u8; 7] {
    match ch {
        'A' => [14,17,17,31,17,17,17], 'B' => [30,17,17,30,17,17,30],
        'C' => [14,17,16,16,16,17,14], 'D' => [30,17,17,17,17,17,30],
        'E' => [31,16,16,30,16,16,31], 'F' => [31,16,16,30,16,16,16],
        'G' => [14,17,16,23,17,17,15], 'H' => [17,17,17,31,17,17,17],
        'I' => [31,4,4,4,4,4,31], 'J' => [7,2,2,2,18,18,12],
        'K' => [17,18,20,24,20,18,17], 'L' => [16,16,16,16,16,16,31],
        'M' => [17,27,21,21,17,17,17], 'N' => [17,25,21,19,17,17,17],
        'O' => [14,17,17,17,17,17,14], 'P' => [30,17,17,30,16,16,16],
        'Q' => [14,17,17,17,21,18,13], 'R' => [30,17,17,30,20,18,17],
        'S' => [15,16,16,14,1,1,30], 'T' => [31,4,4,4,4,4,4],
        'U' => [17,17,17,17,17,17,14], 'V' => [17,17,17,17,17,10,4],
        'W' => [17,17,17,21,21,21,10], 'X' => [17,17,10,4,10,17,17],
        'Y' => [17,17,10,4,4,4,4], 'Z' => [31,1,2,4,8,16,31],
        '0' => [14,17,19,21,25,17,14], '1' => [4,12,4,4,4,4,14],
        '2' => [14,17,1,2,4,8,31], '3' => [30,1,1,14,1,1,30],
        '4' => [2,6,10,18,31,2,2], '5' => [31,16,16,30,1,1,30],
        '6' => [14,16,16,30,17,17,14], '7' => [31,1,2,4,8,8,8],
        '8' => [14,17,17,14,17,17,14], '9' => [14,17,17,15,1,1,14],
        _ => [0; 7],
    }
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
    *status = "Abriendo selector de archivos...".to_string();
    let Some(path) = pick_model_file() else {
        *status = "No se seleccionó archivo. Revisa la terminal si el selector no está disponible.".to_string();
        eprintln!("[OPEN] No se seleccionó ningún archivo.");
        return;
    };
    eprintln!("[OPEN] Archivo seleccionado: {}", path.display());
    open_path(scene, &path, selected, face, status);
}

/// Selects a model path. On Linux we try common desktop pickers explicitly
/// (Zenity/KDialog/YAD), then rfd's XDG portal backend, and finally a terminal
/// prompt. This last fallback is important on WSL/minimal Linux desktops where
/// no XDG portal, DBus service or Zenity package is installed.
fn pick_model_file() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        let filters = "3D Models | *.obj *.gltf *.glb *.r3d *.json\nAll files | *";
        let zenity_args = [
            "--file-selection",
            "--title=Abrir modelo o proyecto 3D",
            "--file-filter=3D Models | *.obj *.gltf *.glb *.r3d *.json",
            "--file-filter=All files | *",
        ];
        match run_picker("zenity", &zenity_args) {
            PickerAttempt::Selected(path) => return Some(path),
            PickerAttempt::Cancelled => return None,
            PickerAttempt::Failed(error) => eprintln!("[OPEN] Zenity: {error}"),
            PickerAttempt::Unavailable => {}
        }

        let kdialog_args = ["--getopenfilename", ".", filters, "--title", "Abrir modelo o proyecto 3D"];
        match run_picker("kdialog", &kdialog_args) {
            PickerAttempt::Selected(path) => return Some(path),
            PickerAttempt::Cancelled => return None,
            PickerAttempt::Failed(error) => eprintln!("[OPEN] KDialog: {error}"),
            PickerAttempt::Unavailable => {}
        }

        let yad_args = [
            "--file",
            "--title=Abrir modelo o proyecto 3D",
            "--file-filter=3D Models | *.obj *.gltf *.glb *.r3d *.json",
            "--file-filter=All files | *",
        ];
        match run_picker("yad", &yad_args) {
            PickerAttempt::Selected(path) => return Some(path),
            PickerAttempt::Cancelled => return None,
            PickerAttempt::Failed(error) => eprintln!("[OPEN] YAD: {error}"),
            PickerAttempt::Unavailable => {}
        }

        // Keep rfd as a portal-based fallback if the desktop has a portal but
        // none of the standalone picker commands is installed.
        if let Some(path) = native_rfd_picker() {
            return Some(path);
        }
        return terminal_path_prompt();
    }

    #[cfg(not(target_os = "linux"))]
    {
        native_rfd_picker()
    }
}

fn native_rfd_picker() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Abrir modelo o proyecto 3D")
        .add_filter("Modelos 3D", &["obj", "gltf", "glb"])
        .add_filter("Proyecto Rust 3D", &["r3d", "json"])
        .pick_file()
}

#[cfg(target_os = "linux")]
enum PickerAttempt {
    Selected(PathBuf),
    Cancelled,
    Unavailable,
    Failed(String),
}

#[cfg(target_os = "linux")]
fn run_picker(program: &str, args: &[&str]) -> PickerAttempt {
    let output = match Command::new(program).args(args).output() {
        Ok(output) => output,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return PickerAttempt::Unavailable,
        Err(error) => return PickerAttempt::Failed(error.to_string()),
    };
    if output.status.success() {
        let selected = String::from_utf8_lossy(&output.stdout).trim().trim_matches('"').to_string();
        if selected.is_empty() {
            return PickerAttempt::Failed("el selector no devolvió una ruta".to_string());
        }
        return PickerAttempt::Selected(PathBuf::from(selected));
    }
    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    // Exit code 1 with no error text is the normal cancel result. Some Linux
    // desktops also return 1 when DISPLAY/DBus is unavailable, so those cases
    // must continue to the next picker/fallback instead of being mistaken for
    // a user cancellation.
    if output.status.code() == Some(1) && detail.is_empty() {
        return PickerAttempt::Cancelled;
    }
    PickerAttempt::Failed(if detail.is_empty() {
        format!("el proceso terminó con {:?}", output.status.code())
    } else {
        detail
    })
}

#[cfg(target_os = "linux")]
fn terminal_path_prompt() -> Option<PathBuf> {
    eprintln!("\n[OPEN] No encontré un selector gráfico disponible (Zenity/KDialog/YAD/XDG Portal).");
    eprintln!("[OPEN] Escribe la ruta completa del archivo OBJ, GLTF, GLB o R3D.");
    eprintln!("[OPEN] Ejemplo: /home/usuario/modelos/cubo.obj");
    print!("Ruta del modelo (Enter para cancelar): ");
    let _ = io::stdout().flush();
    let mut input = String::new();
    if io::stdin().read_line(&mut input).ok()? == 0 {
        eprintln!("[OPEN] No hay entrada de terminal. Puedes abrirlo así: cargo run --release -- /ruta/modelo.obj");
        return None;
    }
    let value = input.trim().trim_matches('"').trim_matches('\'');
    if value.is_empty() {
        return None;
    }
    let path = PathBuf::from(value);
    if !path.is_file() {
        eprintln!("[OPEN] El archivo no existe o no es un archivo: {}", path.display());
        return None;
    }
    Some(path)
}

fn open_path(scene: &mut Scene, path: &Path, selected: &mut Option<usize>, face: &mut Option<(usize, usize)>, status: &mut String) {
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    if extension == "r3d" || extension == "json" {
        match project::load_scene(path) {
            Ok(loaded) => {
                *scene = loaded;
                *selected = if scene.objects.is_empty() { None } else { Some(0) };
                *face = None;
                *status = format!("Proyecto abierto: {}", path.display());
            }
            Err(error) => *status = format!("Error al abrir proyecto: {error:#}"),
        }
    } else {
        // Import into a temporary scene first. If the file is invalid, keep the
        // current scene on screen rather than clearing it before the error is
        // known. One OBJ/GLTF may contain many objects/primitives; retain all.
        let mut imported_scene = Scene::default();
        match importer::import_into_scene(&mut imported_scene, path) {
            Ok(()) => {
                *scene = imported_scene;
                *selected = if scene.objects.is_empty() { None } else { Some(0) };
                *face = None;
                let (loaded, referenced) = texture_counts(scene);
                *status = format!("Modelo importado: {} | texturas: {loaded}/{referenced}", path.display());
                eprintln!("[IMPORT] {} | {} objetos | texturas {loaded}/{referenced}", path.display(), scene.objects.len());
            }
            Err(error) => {
                *status = format!("Error al importar: {error:#}");
                eprintln!("[IMPORT][ERROR] {}: {error:#}", path.display());
            }
        }
    }
}

fn load_demo(scene: &mut Scene, selected: &mut Option<usize>, face: &mut Option<(usize, usize)>, status: &mut String) {
    *scene = Scene::default();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples").join("models").join("demo.obj");
    match importer::import_into_scene(scene, &path) {
        Ok(()) => {
            *selected = if scene.objects.is_empty() { None } else { Some(0) };
            *face = None;
            let (loaded, referenced) = texture_counts(scene);
            *status = format!("DEMO cargado | texturas cargadas: {loaded}/{referenced} (checker.png)");
        }
        Err(error) => *status = format!("No se pudo cargar DEMO ({}): {error:#}", path.display()),
    }
}

fn texture_counts(scene: &Scene) -> (usize, usize) {
    let mut loaded = 0usize;
    let mut referenced = 0usize;
    for object in &scene.objects {
        for material in &object.mesh.materials {
            if material.texture_source.is_some() { referenced += 1; }
            if material.texture.is_some() { loaded += 1; }
        }
    }
    (loaded, referenced)
}

fn export_all(scene: &Scene, renderer: &Renderer, last_export: &mut Option<PathBuf>, status: &mut String) {
    match export::export_bundle(scene, &renderer.color, renderer.width, renderer.height) {
        Ok(path) => {
            *status = format!("Exportación completa guardada en {}", path.display());
            *last_export = Some(path);
        }
        Err(error) => *status = format!("Error en exportación: {error:#}"),
    }
}

fn save_capture(scene: &Scene, renderer: &Renderer, last_export: &mut Option<PathBuf>, status: &mut String) {
    if last_export.is_none() {
        match export::export_bundle(scene, &renderer.color, renderer.width, renderer.height) {
            Ok(path) => *last_export = Some(path),
            Err(error) => { *status = format!("No se pudo crear carpeta de exportación: {error:#}"); return; }
        }
    }
    let Some(folder) = last_export.as_ref() else { return; };
    match export::save_capture_in_export(folder, &renderer.color, renderer.width, renderer.height) {
        Ok(path) => *status = format!("Captura guardada: {}", path.display()),
        Err(error) => *status = format!("Error al guardar captura: {error:#}"),
    }
}
