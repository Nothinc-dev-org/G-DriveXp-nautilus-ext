//! Implementación del InfoProvider para G-DriveXP
//!
//! Registra un GType que implementa NautilusInfoProvider y consulta
//! el estado de sincronización vía IPC.

use crate::ffi::*;
use crate::ipc_client::IpcClient;
use gobject_sys::{GObject, GTypeInfo, GInterfaceInfo, GTypeModule, g_type_module_register_type, g_type_module_add_interface};
use glib_sys::GType;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;
use crossbeam_channel::{bounded, Sender, Receiver};

// ============================================================
// IPC Worker Thread Architecture
// ============================================================

/// Request to the IPC worker
struct IpcRequest {
    uri: String,
    response_tx: Sender<crate::SyncStatus>,
}

/// IPC worker that runs a dedicated thread with multi-threaded Tokio runtime
struct IpcWorker {
    request_tx: Sender<IpcRequest>,
}

impl IpcWorker {
    fn new() -> Self {
        let (request_tx, request_rx): (Sender<IpcRequest>, Receiver<IpcRequest>) = bounded(32);
        
        // Spawn dedicated worker thread
        thread::spawn(move || {
            // Single-threaded runtime is sufficient here because:
            // 1. This runs in its own dedicated thread (not Nautilus main thread)
            // 2. Requests are processed sequentially from the channel
            // 3. More lightweight than multi-threaded runtime
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create IPC worker runtime");
            
            rt.block_on(async {
                let client = IpcClient::new();
                
                while let Ok(req) = request_rx.recv() {
                    // Query IPC with timeout
                    let status = match tokio::time::timeout(
                        Duration::from_millis(100),
                        client.get_file_status(&req.uri)
                    ).await {
                        Ok(Ok(status)) => status,
                        Ok(Err(_)) => crate::SyncStatus::Unknown,
                        Err(_) => crate::SyncStatus::Unknown, // Timeout
                    };
                    
                    // Send response back (ignore error if receiver dropped)
                    let _ = req.response_tx.send(status);
                }
            });
        });
        
        Self { request_tx }
    }
    
    /// Query file status with timeout from main thread
    fn query_status(&self, uri: &str, timeout: Duration) -> crate::SyncStatus {
        let (response_tx, response_rx) = bounded(1);
        
        let request = IpcRequest {
            uri: uri.to_string(),
            response_tx,
        };
        
        // Send request to worker
        if self.request_tx.send(request).is_err() {
            return crate::SyncStatus::Unknown;
        }
        
        // Wait for response with timeout
        match response_rx.recv_timeout(timeout) {
            Ok(status) => status,
            Err(_) => crate::SyncStatus::Unknown,
        }
    }
}

// Global IPC worker instance
static IPC_WORKER: OnceLock<IpcWorker> = OnceLock::new();

/// Public API for querying IPC status (used by menu_provider)
pub fn ipc_query_status(uri: &str) -> Result<crate::SyncStatus, ()> {
    let worker = IPC_WORKER.get_or_init(IpcWorker::new);
    Ok(worker.query_status(uri, Duration::from_millis(50)))
}

// ============================================================

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
    
    // Consultar estado usando worker (no bloquea el main thread más de 50ms)
    let worker = IPC_WORKER.get_or_init(IpcWorker::new);
    let status = worker.query_status(&uri, Duration::from_millis(50));
    
    // Aplicar emblema según estado
    match status {
        crate::SyncStatus::Synced => {
            // Verde: sincronizado (local + drive)
            let emblem = str_to_cstring("emblem-gdrivexp-synced");
            nautilus_file_info_add_emblem(file, emblem.as_ptr());
        }
        crate::SyncStatus::CloudOnly => {
            // Azul: solo en drive
            let emblem = str_to_cstring("emblem-gdrivexp-cloud");
            nautilus_file_info_add_emblem(file, emblem.as_ptr());
        }
        crate::SyncStatus::LocalOnly => {
            // Naranja: solo local (pendiente de subir)
            let emblem = str_to_cstring("emblem-gdrivexp-local");
            nautilus_file_info_add_emblem(file, emblem.as_ptr());
        }
        crate::SyncStatus::Error => {
            // Rojo: error
            let emblem = str_to_cstring("emblem-gdrivexp-error");
            nautilus_file_info_add_emblem(file, emblem.as_ptr());
        }
        crate::SyncStatus::Unknown => {
            // Sin emblema
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
    
    // Registrar NautilusMenuProvider
    let menu_iface_info = GInterfaceInfo {
        interface_init: Some(menu_provider_iface_init),
        interface_finalize: None,
        interface_data: std::ptr::null_mut(),
    };
    
    g_type_module_add_interface(
        module,
        GDRIVEXP_PROVIDER_TYPE,
        nautilus_menu_provider_get_type(),
        &menu_iface_info,
    );
}

unsafe extern "C" fn menu_provider_iface_init(
    iface: glib_sys::gpointer,
    _data: glib_sys::gpointer,
) {
    let iface = iface as *mut NautilusMenuProviderInterface;
    (*iface).get_file_items = Some(crate::menu_provider::get_file_items_impl);
    (*iface).get_background_items = None; // No implementamos background items
}

pub fn get_type() -> GType {
    unsafe { GDRIVEXP_PROVIDER_TYPE }
}
