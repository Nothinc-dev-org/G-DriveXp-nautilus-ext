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
    response_tx: Sender<crate::FileStatusData>,
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
            crate::log_debug("Worker thread started");
            let result = std::panic::catch_unwind(move || {
                // Single-threaded runtime is sufficient here because:
                // 1. This runs in its own dedicated thread (not Nautilus main thread)
                // 2. Requests are processed sequentially from the channel
                // 3. More lightweight than multi-threaded runtime
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create IPC worker runtime");
                
                rt.block_on(async {
                    crate::log_debug("Worker LocalSet started");
                    let client = IpcClient::new();
                    
                    while let Ok(req) = request_rx.recv() {
                        crate::log_debug(&format!("Worker received request: {}", req.uri));
                        // Query IPC with timeout
                        let status_data = match tokio::time::timeout(
                            Duration::from_millis(100),
                            client.get_extended_status(&req.uri)
                        ).await {
                            Ok(Ok(data)) => data,
                            Ok(Err(e)) => {
                                crate::log_debug(&format!("Client Error: {}", e));
                                crate::FileStatusData {
                                    status: crate::SyncStatus::Unknown,
                                    availability: crate::FileAvailability::NotTracked,
                                    is_shared: false,
                                }
                            },
                            Err(_) => {
                                crate::log_debug("Worker timeout");
                                crate::FileStatusData {
                                    status: crate::SyncStatus::Unknown,
                                    availability: crate::FileAvailability::NotTracked,
                                    is_shared: false,
                                }
                            }
                        };
                        
                        // Send response back (ignore error if receiver dropped)
                        let _ = req.response_tx.send(status_data);
                    }
                    crate::log_debug("Worker channel closed");
                });
            });
            
            if let Err(e) = result {
                crate::log_debug(&format!("WORKER PANIC: {:?}", e));
            }
        });
        
        Self { request_tx }
    }
    
    /// Query file status with timeout from main thread
    fn query_extended_status(&self, uri: &str, timeout: Duration) -> crate::FileStatusData {
        let (response_tx, response_rx) = bounded(1);
        
        let request = IpcRequest {
            uri: uri.to_string(),
            response_tx,
        };
        
        // Send request to worker
        if self.request_tx.send(request).is_err() {
            return crate::FileStatusData {
                status: crate::SyncStatus::Unknown,
                availability: crate::FileAvailability::NotTracked,
                is_shared: false,
            };
        }
        
        // Wait for response with timeout
        match response_rx.recv_timeout(timeout) {
            Ok(data) => data,
            Err(_) => crate::FileStatusData {
                status: crate::SyncStatus::Unknown,
                availability: crate::FileAvailability::NotTracked,
                is_shared: false,
            },
        }
    }
}

// Global IPC worker instance
static IPC_WORKER: OnceLock<IpcWorker> = OnceLock::new();

/// Public API for querying IPC status (used by menu_provider)
pub fn ipc_query_status(uri: &str) -> Result<crate::SyncStatus, ()> {
    let worker = IPC_WORKER.get_or_init(IpcWorker::new);
    Ok(worker.query_extended_status(uri, Duration::from_millis(50)).status)
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

    crate::log_debug(&format!("update_file_info_impl called for: {}", uri));
    
    // Solo procesar archivos file://
    if !uri.starts_with("file://") {
        return NautilusOperationResult::Complete;
    }
    
    // Consultar estado usando worker (no bloquea el main thread más de 50ms)
    let worker = IPC_WORKER.get_or_init(IpcWorker::new);
    let data = worker.query_extended_status(&uri, Duration::from_millis(50));
    
    // NUEVO: Aplicar emblema de compartido si corresponde
    // Se añade primero para que quede visualmente "abajo" del emblema de estado (el último añadido queda arriba)
    if data.is_shared {
        let emblem = str_to_cstring("emblem-shared");
        nautilus_file_info_add_emblem(file, emblem.as_ptr());
    }

    // Aplicar emblema según estado de sincronización
    match data.status {
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

// ============================================================
// Inicialización de la interface
// ============================================================

unsafe extern "C" fn info_provider_iface_init(iface: glib_sys::gpointer, _data: glib_sys::gpointer) {
    crate::log_debug("info_provider_iface_init called");
    let iface = iface as *mut NautilusInfoProviderInterface;
    (*iface).update_file_info = Some(update_file_info_impl);
    (*iface).cancel_update = Some(cancel_update_impl);
    crate::log_debug("info_provider_iface_init finished");
}

unsafe extern "C" fn class_init(class: glib_sys::gpointer, _data: glib_sys::gpointer) {
    crate::log_debug("class_init called");
    // Peek parent class just to be sure we touch it and compiler doesn't optimize away
    let parent = gobject_sys::g_type_class_peek_parent(class);
    if !parent.is_null() {
        crate::log_debug("class_init: parent class found");
    } else {
        crate::log_debug("class_init: parent class is null (unexpected for GObject derived)");
    }
}

unsafe extern "C" fn instance_init(_instance: *mut gobject_sys::GTypeInstance, _class: glib_sys::gpointer) {
    crate::log_debug("instance_init called");
}

// ============================================================
// Registro del tipo con GObject
// ============================================================

pub unsafe fn register_type(module: *mut GTypeModule) {
    // Debug: Check parent type validity
    let parent_type = gobject_sys::g_object_get_type();
    crate::log_debug(&format!("Parent GType (GObject): {}", parent_type));

    // Intentar un nombre único para evitar colisiones con versiones anteriores cargadas en memoria
    let type_name = str_to_cstring("GDriveXPProviderFixed");
    
    // Debug size
    crate::log_debug(&format!("sizeof(GTypeInfo) = {}", std::mem::size_of::<GTypeInfo>()));
    crate::log_debug(&format!("sizeof(GDriveXPProviderClass) = {}", std::mem::size_of::<GDriveXPProviderClass>()));
    crate::log_debug(&format!("sizeof(GDriveXPProvider) = {}", std::mem::size_of::<GDriveXPProvider>()));

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
        parent_type,
        type_name.as_ptr(),
        &type_info,
        0, // GTypeFlags (u32)
    );
    
    let gtype_val = GDRIVEXP_PROVIDER_TYPE;
    crate::log_debug(&format!("Registered GType: {}", gtype_val));
    
    if GDRIVEXP_PROVIDER_TYPE == 0 {
        crate::log_debug("CRITICAL: Failed to register GType! (Name collision or invalid parent?)");
        return;
    }

    // Info de la interface NautilusInfoProvider
    let iface_info = GInterfaceInfo {
        interface_init: Some(info_provider_iface_init),
        interface_finalize: None,
        interface_data: std::ptr::null_mut(),
    };
    
    // Registrar que implementamos NautilusInfoProvider
    let info_type = nautilus_info_provider_get_type();
    crate::log_debug(&format!("NautilusInfoProvider Type: {}", info_type));
    
    g_type_module_add_interface(
        module,
        GDRIVEXP_PROVIDER_TYPE,
        info_type,
        &iface_info,
    );
    
    // Registrar NautilusMenuProvider
    let menu_iface_info = GInterfaceInfo {
        interface_init: Some(menu_provider_iface_init),
        interface_finalize: None,
        interface_data: std::ptr::null_mut(),
    };
    
    let menu_type = nautilus_menu_provider_get_type();
    crate::log_debug(&format!("NautilusMenuProvider Type: {}", menu_type));

    g_type_module_add_interface(
        module,
        GDRIVEXP_PROVIDER_TYPE,
        menu_type,
        &menu_iface_info,
    );
}

unsafe extern "C" fn menu_provider_iface_init(
    iface: glib_sys::gpointer,
    _data: glib_sys::gpointer,
) {
    crate::log_debug("menu_provider_iface_init called");
    let iface = iface as *mut NautilusMenuProviderInterface;
    (*iface).get_file_items = Some(crate::menu_provider::get_file_items_impl);
    (*iface).get_background_items = None; // No implementamos background items
}

pub fn get_type() -> GType {
    unsafe { GDRIVEXP_PROVIDER_TYPE }
}
