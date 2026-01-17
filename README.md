# G-DriveXP Nautilus Extension

Extensión para el administrador de archivos Nautilus (GNOME) que proporciona indicadores visuales del estado de sincronización para G-DriveXP.

Esta extensión permite visualizar en tiempo real qué archivos están sincronizados, en proceso de subida o presentan errores, integrándose nativamente en la interfaz de GNOME.

## ✨ Características

- **Emblemas de Estado**: Iconos superpuestos que indican el estado de cada archivo dentro del punto de montaje.
- **Integración Nativa**: Escrito en Rust utilizando FFI para interactuar directamente con las APIs de `libnautilus-extension`.
- **Comunicación Eficiente**: Utiliza un cliente IPC ligero para obtener estados desde el daemon de G-DriveXP sin penalización de rendimiento.
- **Detección Automática**: Solo se activa para rutas dentro del punto de montaje configurado.

## 🟢 Estados Soportados

| Emblema | Significado |
| :---: | :--- |
| `v` | **Sincronizado**: El archivo coincide con la versión en la nube. |
| `~` | **Sincronizando**: El archivo se está subiendo o descargando. |
| `.` | **Pendiente**: Cambios detectados esperando turno de subida. |
| `x` | **Error**: Problema de permisos o conflicto de sincronización. |

## 🛠️ Requisitos

- `libnautilus-extension` (cabeceras de desarrollo)
- `pkg-config`
- `glib2` (cabeceras de desarrollo)
- Rust (stable)

En Fedora:
```bash
sudo dnf install nautilus-devel glib2-devel
```

## 🚀 Instalación y Compilación

1. **Compilar la extensión**:
   ```bash
   cargo build --release
   ```

2. **Instalar la librería compartida**:
   Crea el directorio de extensiones si no existe y copia el binario:
   ```bash
   mkdir -p ~/.local/share/nautilus/extensions-4/
   cp target/release/libnautilus_ext.so ~/.local/share/nautilus/extensions-4/
   ```

3. **Reiniciar Nautilus**:
   Para aplicar los cambios, Nautilus debe reiniciarse por completo:
   ```bash
   nautilus -q
   ```

## 🏗️ Arquitectura

La extensión funciona como un cliente pasivo:
1. `InfoProvider`: Nautilus solicita información para cada archivo visible.
2. `IPC Client`: La extensión consulta al socket de G-DriveXP (`/run/user/UID/gdrivexp.sock`).
3. `Emblems`: Basado en la respuesta, se asigna el emblema correspondiente de forma asíncrona.

---
*Parte del ecosistema G-DriveXP.*
