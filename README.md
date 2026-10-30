# Rust 3D Editor (prototipo funcional)

Visor y pintor 3D por software escrito en Rust. Usa `minifb` para la ventana y el framebuffer, `nalgebra-glm` para matrices/cámara, `tobj` para OBJ/MTL, `gltf` para glTF/GLB e `image` para texturas y capturas.

> Este es un primer entregable funcional del núcleo del editor. Es un renderizador por software, no una aplicación acelerada por GPU. Los modelos muy grandes pueden ir lentos.

## Requisitos

- Rust estable (recomendado instalar con [rustup](https://rustup.rs/)).
- En Linux, las dependencias gráficas que requiera `minifb`/X11 o Wayland de tu distribución.

## Ejecutar

Desde esta carpeta:

```bash
cargo run --release
```

La ventana inicia con un cubo de ejemplo. Pulsa `O` para abrir un modelo.

## Controles

| Control | Acción |
|---|---|
| `O` | Abrir `.obj`, `.gltf`, `.glb` o proyecto `.r3d` |
| `C` | Cambiar a la siguiente cámara |
| `V` | Crear una cámara a partir de la cámara activa |
| Clic derecho + arrastrar | Orbitar cámara |
| Clic central + arrastrar | Desplazar cámara (pan) |
| Rueda del mouse | Zoom |
| `F` | Encuadrar la escena |
| `Home` | Restablecer cámara |
| Clic izquierdo | Seleccionar triángulo |
| `P` | Activar/desactivar modo pintura |
| `1` a `4` | Elegir rojo, verde, azul o amarillo |
| `5` | Blanco: volver a material/textura original |
| `Delete` | Quitar pintura de la cara seleccionada |
| `W` | Mostrar/ocultar aristas triangulares |
| `T` | Activar/desactivar texturas |
| Flechas + `PageUp`/`PageDown` | Mover en X/Y/Z |
| `M` + flechas/PageUp/PageDown | Mover el objetivo de la cámara |
| `G` + flechas/PageUp/PageDown | Mover toda la escena |
| `L` + flechas/PageUp/PageDown | Ajustar la dirección de la luz direccional |
| `Shift` + flechas/PageUp/PageDown | Mover el objetivo de la cámara temporalmente |
| `Alt` + flechas/PageUp/PageDown | Movimiento fino |
| `Ctrl+S` | Guardar proyecto `.r3d` |
| `F12` | Guardar captura PNG |
| `Esc` | Salir |

Para pintar: pulsa `P`, selecciona un color con `1`–`4` y haz clic sobre una cara triangular. La selección actual se resalta en amarillo.

## Formatos y comportamiento

- **OBJ/MTL:** importa posiciones, coordenadas UV, triangulación, colores difusos y texturas difusas indicadas en MTL. Los archivos de textura se buscan relativos al OBJ/MTL.
- **glTF/GLB:** importa primitivas triangulares, UV0, color base y textura base-color, incluidas imágenes embebidas habituales. Las primitivas se importan como objetos separados.
- **`.r3d`:** guarda geometría importada, materiales, píxeles de texturas, colores pintados, transformaciones y cámaras en JSON legible. La extensión es propia, el contenido es JSON.
- **PNG:** guarda el framebuffer que se está mostrando.

## Limitaciones conocidas de esta primera versión

1. El importador glTF recoge mallas/primitivas y materiales, pero todavía no aplica las transformaciones jerárquicas de nodos a las mallas, ni soporta skins, morph targets o animaciones.
2. Se importan cámaras referenciadas por nodos glTF y su transformación local y campo de visión; todavía no se acumulan transformaciones de nodos padre. Las cámaras ortográficas se aproximan como perspectiva. También se pueden duplicar con `V` y cambiar con `C`.
3. Se modela una luz direccional global. Las luces puntuales/spot del archivo aún no se importan.
4. La rasterización no hace clipping geométrico completo contra los planos near/far; los triángulos que cruzan el plano cercano pueden desaparecer.
5. La pintura es un color por cara, almacenado como overlay; no modifica la textura UV original.
6. No hay panel gráfico de jerarquía/inspector: las operaciones se hacen con teclado y mouse para mantener la interfaz mínima.
7. No hay exportación de vuelta a OBJ/GLTF todavía; el formato editable es `.r3d`.

## Estructura

```text
src/
  main.rs       bucle de ventana, controles y diálogos
  model.rs      escena, objetos, triángulos, materiales y cámaras
  importer.rs   importadores OBJ/MTL y glTF/GLB
  renderer.rs   proyección, z-buffer, rasterización, texturas y picking
  project.rs    guardar/cargar .r3d y capturas PNG
```

## Solución de problemas

- Si una textura OBJ no aparece, revisa que exista y que la ruta de `map_Kd` en el `.mtl` sea relativa al archivo OBJ/MTL.
- Si un glTF usa una extensión o material no estándar, prueba primero exportarlo desde Blender como glTF 2.0 estándar o GLB.
- Para modelos muy densos, reduce el número de polígonos antes de abrirlos: la rasterización por software prioriza claridad didáctica y facilidad de modificación, no velocidad de producción.
