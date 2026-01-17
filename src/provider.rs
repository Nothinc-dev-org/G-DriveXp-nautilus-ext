//! Implementación del InfoProvider para G-DriveXP
//!
//! Registra un GType que implementa NautilusInfoProvider y consulta
//! el estado de sincronización vía IPC.

use crate::ffi::*;
use crate::ipc_client::IpcClient;
use gobject_sys::{GObject, GTypeInfo, GInterfaceInfo, GTypeModule};
use glib_sys::GType;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

// Runtime de Tokio global para operaciones async
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static IPC_CLIENT: OnceLock<IpcClient> = OnceLock::new();

/// GType de nuestra extensión (se registra en nautilus_module_initialize)
static mut GDRIVEXP_PROVIDER_TYPE: GType = 0;

// ============================================================
// Struct que representa nuestra extensión (hereda de GObject)
// ============================================================

#[repr(C)]
pub struct GDriveXPProvider {
    parent: GObject,
}

#[repr(C)]
pub struct GDriveXPProviderClass {
    parent_class: gobject_sys::GObjectClass,
}

// ============================================================
// Implementación de update_file_info
// ============================================================

unsafe extern "C" fn update_file_info_impl(
    _provider: *mut GObject,
    file: *mut NautilusFileInfo,
    _update_complete: *mut gobject_sys::GClosure,
    _handle: *mut *mut NautilusOperationHandle,
) -> NautilusOperationResult {
    // Obtener URI del archivo
    let uri_ptr = nautilus_file_info_get_uri(file);
    let uri = match gchar_to_string_free(uri_ptr) {
        Some(u) => u,
        None => return NautilusOperationResult::Complete,
    };
    
    // Solo procesar archivos file://
    if !uri.starts_with("file://") {
        return NautilusOperationResult::Complete;
    }
    
    // Consultar estado de sincronización
    let rt = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime")
    });
    
    let client = IPC_CLIENT.get_or_init(IpcClient::new);
    
    let status = rt.block_on(async {
        client.get_file_status(&uri).await
    });
    
    // Aplicar emblema según estado
    match status {
        Ok(crate::SyncStatus::Synced) => {
            let emblem = str_to_cstring("emblem-default");
            nautilus_file_info_add_emblem(file, emblem.as_ptr());
        }
        Ok(crate::SyncStatus::Pending) => {
            let emblem = str_to_cstring("emblem-important");
            nautilus_file_info_add_emblem(file, emblem.as_ptr());
        }
        Ok(crate::SyncStatus::Syncing) => {
            let emblem = str_to_cstring("emblem-synchronizing");
            nautilus_file_info_add_emblem(file, emblem.as_ptr());
        }
        Ok(crate::SyncStatus::Error) => {
            let emblem = str_to_cstring("emblem-unreadable");
            nautilus_file_info_add_emblem(file, emblem.as_ptr());
        }
        Ok(crate::SyncStatus::Unknown) | Err(_) => {
            // No añadir emblema
        }
    }
    
    NautilusOperationResult::Complete
}

unsafe extern "C" fn cancel_update_impl(
    _provider: *mut GObject,
    _handle: *mut NautilusOperationHandle,
) {
    // No-op: nuestras operaciones son síncronas
}

// ============================================================
// Inicialización de la interface
// ============================================================

unsafe extern "C" fn info_provider_iface_init(iface: glib_sys::gpointer, _data: glib_sys::gpointer) {
    let iface = iface as *mut NautilusInfoProviderInterface;
    (*iface).update_file_info = Some(update_file_info_impl);
    (*iface).cancel_update = Some(cancel_update_impl);
}

unsafe extern "C" fn class_init(_class: glib_sys::gpointer, _data: glib_sys::gpointer) {
    // No-op: no necesitamos inicialización de clase
}

unsafe extern "C" fn instance_init(_instance: *mut gobject_sys::GTypeInstance, _class: glib_sys::gpointer) {
    // No-op: no necesitamos inicialización de instancia
}

// ============================================================
// Registro del tipo con GObject
// ============================================================

pub unsafe fn register_type(module: *mut GTypeModule) {
    let type_name = str_to_cstring("GDriveXPProvider");
    
    // Info del tipo
    let type_info = GTypeInfo {
        class_size: std::mem::size_of::<GDriveXPProviderClass>() as u16,
        base_init: None,
        base_finalize: None,
        class_init: Some(class_init),
        class_finalize: None,
        class_data: std::ptr::null(),
        instance_size: std::mem::size_of::<GDriveXPProvider>() as u16,
        n_preallocs: 0,
        instance_init: Some(instance_init),
        value_table: std::ptr::null(),
    };
    
    // Registrar tipo derivado de GObject
    GDRIVEXP_PROVIDER_TYPE = g_type_module_register_type(
        module,
        gobject_sys::g_object_get_type(),
        type_name.as_ptr(),
        &type_info,
        0, // GTypeFlags (u32)
    );
    
    // Info de la interface NautilusInfoProvider
    let iface_info = GInterfaceInfo {
        interface_init: Some(info_provider_iface_init),
        interface_finalize: None,
        interface_data: std::ptr::null_mut(),
    };
    
    // Registrar que implementamos NautilusInfoProvider
    g_type_module_add_interface(
        module,
        GDRIVEXP_PROVIDER_TYPE,
        nautilus_info_provider_get_type(),
        &iface_info,
    );
}

pub fn get_type() -> GType {
    unsafe { GDRIVEXP_PROVIDER_TYPE }
}
