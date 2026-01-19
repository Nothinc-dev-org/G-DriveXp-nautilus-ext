//! Extensión de Nautilus para G-DriveXP (GTK4 / libnautilus-extension-4)
//!
//! Muestra emblemas de sincronización en archivos montados por G-DriveXP.

mod ffi;
mod ipc_client;
mod provider;
pub mod menu_provider;

use glib_sys::GType;
use gobject_sys::GTypeModule;
use std::os::raw::c_int;

/// Estado de sincronización (debe coincidir con src/ipc/mod.rs del daemon)
/// - Synced: Local + Drive (verde)
/// - CloudOnly: Solo en Drive, no descargado (azul)
/// - LocalOnly: Solo local, pendiente de subir (naranja)
/// - Error: Error de sincronización (rojo)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SyncStatus {
    Synced,      // Verde: en local y en drive
    CloudOnly,   // Azul: solo en drive
    LocalOnly,   // Naranja: solo local (pending upload)
    Error,       // Rojo: error de sincronización
    Unknown,     // Sin emblema
}

/// Disponibilidad de un archivo (debe coincidir con src/ipc/mod.rs del daemon)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FileAvailability {
    LocalOnline,
    OnlineOnly,
    NotTracked,
}

// ============================================================
// Funciones exportadas requeridas por Nautilus
// ============================================================

// Helper de logging
fn log_debug(msg: &str) {
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/gdrivexp-nautilus-init.log")
    {
        let _ = writeln!(file, "{}", msg);
    }
}

/// Llamada cuando la extensión es cargada
#[no_mangle]
pub unsafe extern "C" fn nautilus_module_initialize(module: *mut GTypeModule) {
    log_debug("nautilus_module_initialize called");
    // Registrar nuestro tipo GDriveXPProvider
    provider::register_type(module);
    log_debug("provider registered");
}

/// Llamada cuando la extensión es descargada
#[no_mangle]
pub unsafe extern "C" fn nautilus_module_shutdown() {
    log_debug("nautilus_module_shutdown called");
    // Cleanup si es necesario
}

/// Nautilus llama esto para obtener los tipos que exportamos
#[no_mangle]
pub unsafe extern "C" fn nautilus_module_list_types(
    types: *mut *const GType,
    num_types: *mut c_int,
) {
    log_debug("nautilus_module_list_types called");
    static mut TYPE_LIST: [GType; 1] = [0];
    
    TYPE_LIST[0] = provider::get_type();
    
    *types = std::ptr::addr_of!(TYPE_LIST) as *const GType;
    *num_types = 1;
    log_debug("types listed");
}
