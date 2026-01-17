# G-DriveXP Nautilus Extension

Extensión para el administrador de archivos Nautilus (GNOME) que proporciona indicadores visuales del estado de sincronización para G-DriveXP.

Esta extensión permite visualizar en tiempo real qué archivos están sincronizados, pendientes de subida, solo en la nube o con errores, integrándose nativamente en la interfaz de GNOME.

## ✨ Características

- **Emblemas de Estado**: Iconos superpuestos que indican el estado de cada archivo dentro del punto de montaje.
- **Integración Nativa**: Escrito en Rust utilizando FFI para interactuar directamente con las APIs de `libnautilus-extension`.
- **Comunicación Eficiente**: Utiliza un cliente IPC ligero para obtener estados desde el daemon de G-DriveXP sin penalización de rendimiento.
- **Detección Automática**: Solo se activa para rutas dentro del punto de montaje configurado.
- **URL Decoding**: Maneja correctamente nombres de archivo con caracteres especiales (espacios, paréntesis, acentos, etc.).

## 🟢 Estados Soportados

| Emblema | Color | Significado |
| :---: | :--- | :--- |
| ✓ | 🟢 Verde | **Sincronizado**: El archivo existe localmente y coincide con la versión en Drive. |
| ☁️ | 🔵 Azul | **Solo en Drive**: El archivo está en Google Drive pero no ha sido descargado localmente. |
| ! | 🟠 Naranja | **Pendiente**: Cambios locales esperando ser subidos a Drive. |
| ✗ | 🔴 Rojo | **Error**: Problema de permisos o conflicto de sincronización. |

## 🛠️ Requisitos

- `libnautilus-extension` (cabeceras de desarrollo)
- `pkg-config`
- `glib2` (cabeceras de desarrollo)
- Rust (stable)

En Fedora:
```bash
sudo dnf install nautilus-devel glib2-devel
```

## 🚀 Instalación

### 1. Compilar la extensión
```bash
cargo build --release
```

### 2. Instalar los íconos de emblema
```bash
mkdir -p ~/.local/share/icons/hicolor/scalable/emblems/
cp icons/*.svg ~/.local/share/icons/hicolor/scalable/emblems/
gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor/
```

### 3. Instalar la librería compartida
```bash
mkdir -p ~/.local/share/nautilus/extensions-4/
cp target/release/libgdrivexp_nautilus.so ~/.local/share/nautilus/extensions-4/
```

### 4. Reiniciar Nautilus
```bash
nautilus -q && nautilus &
```

## 🔧 Depuración

La extensión incluye un binario de depuración para probar la comunicación IPC:

```bash
cargo run --bin debug_ipc
```

Este comando enviará un ping al daemon de G-DriveXP y mostrará la respuesta.

## 🏗️ Arquitectura

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│     Nautilus    │────▶│  nautilus-ext    │────▶│   g-drive-xp    │
│ (File Manager)  │     │ (InfoProvider)   │     │ (IPC Server)    │
└─────────────────┘     └──────────────────┘     └─────────────────┘
        │                       │                        │
        │  update_file_info()   │  Unix Socket           │
        │◀──────────────────────│  /run/user/UID/        │
        │                       │  gdrivexp.sock         │
        │  add_emblem()         │                        │
        │◀──────────────────────│◀───────────────────────│
        │                       │  SyncStatus            │
```

1. **InfoProvider**: Nautilus solicita información para cada archivo visible.
2. **IPC Client**: La extensión consulta al socket de G-DriveXP (`/run/user/UID/gdrivexp.sock`).
3. **Emblems**: Basado en la respuesta (`Synced`, `CloudOnly`, `LocalOnly`, `Error`), se asigna el emblema correspondiente.

## 📁 Estructura del Proyecto

```
nautilus-ext/
├── Cargo.toml
├── build.rs              # Configuración de pkg-config
├── icons/                # Íconos SVG de emblemas
│   ├── emblem-gdrivexp-synced.svg   (verde)
│   ├── emblem-gdrivexp-cloud.svg    (azul)
│   ├── emblem-gdrivexp-local.svg    (naranja)
│   └── emblem-gdrivexp-error.svg    (rojo)
└── src/
    ├── lib.rs            # Entry point de la extensión
    ├── ffi.rs            # Bindings FFI para libnautilus-extension
    ├── provider.rs       # Implementación de NautilusInfoProvider
    ├── ipc_client.rs     # Cliente IPC para comunicación con daemon
    └── bin/
        └── debug_ipc.rs  # Utilidad de depuración
```

---
*Parte del ecosistema G-DriveXP.*
