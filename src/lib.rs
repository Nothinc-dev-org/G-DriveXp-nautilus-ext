//! Extensión de Nautilus para G-DriveXP (GTK4 / libnautilus-extension-4)
//!
//! Muestra emblemas de sincronización en archivos montados por G-DriveXP.

mod ffi;
mod ipc_client;
mod provider;

use glib_sys::GType;
use gobject_sys::GTypeModule;
use std::os::raw::c_int;

/// Estado de sincronización (debe coincidir con src/ipc/mod.rs del daemon)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub enum SyncStatus {
    Synced,
    Syncing,
    Pending,
    Error,
    Unknown,
}

// ============================================================
// Funciones exportadas requeridas por Nautilus
// ============================================================

/// Llamada cuando la extensión es cargada
#[no_mangle]
pub unsafe extern "C" fn nautilus_module_initialize(module: *mut GTypeModule) {
    // Registrar nuestro tipo GDriveXPProvider
    provider::register_type(module);
}

/// Llamada cuando la extensión es descargada
#[no_mangle]
pub unsafe extern "C" fn nautilus_module_shutdown() {
    // Cleanup si es necesario
}

/// Nautilus llama esto para obtener los tipos que exportamos
#[no_mangle]
pub unsafe extern "C" fn nautilus_module_list_types(
    types: *mut *const GType,
    num_types: *mut c_int,
) {
    static mut TYPE_LIST: [GType; 1] = [0];
    
    TYPE_LIST[0] = provider::get_type();
    
    *types = TYPE_LIST.as_ptr();
    *num_types = 1;
}
