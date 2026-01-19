//! Implementación del MenuProvider para acciones contextuales

use crate::ffi::*;
use crate::ipc_client::IpcClient;
use crate::SyncStatus;
use glib_sys::gpointer;
use gobject_sys::GObject;
use percent_encoding::percent_decode_str;
use std::thread;

// Helper de logging
fn log_debug(msg: &str) {
    eprintln!("G-DriveXP-Ext: {}", msg);
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/gdrivexp-nautilus-v3.log")
    {
        let _ = writeln!(file, "{}", msg);
    }
}

/// Callback para get_file_items
pub unsafe extern "C" fn get_file_items_impl(
    _provider: *mut GObject,
    files: *mut glib_sys::GList,
) -> *mut glib_sys::GList {
    log_debug("v3: get_file_items_impl called");

    // 1. Solo procesar si hay exactamente 1 archivo seleccionado
    let file_count = g_list_length(files);
    if file_count != 1 {
        log_debug(&format!("Ignored: file_count = {}", file_count));
        return std::ptr::null_mut();
    }
    
    // 2. Obtener primer archivo de la lista
    if files.is_null() {
        return std::ptr::null_mut();
    }
    let file = (*files).data as *mut NautilusFileInfo;
    
    // 3. Obtener URI
    let uri_ptr = nautilus_file_info_get_uri(file);
    let uri = match gchar_to_string_free(uri_ptr) {
        Some(u) => u,
        None => return std::ptr::null_mut(),
    };
    
    log_debug(&format!("URI: {}", uri));

    // Solo procesar file://
    if !uri.starts_with("file://") {
        log_debug("Ignored: not file://");
        return std::ptr::null_mut();
    }
    
    // 4. Decodificar URI a Path local
    let path_str = uri.strip_prefix("file://").unwrap_or(&uri);
    let _decoded_path = match percent_decode_str(path_str).decode_utf8() {
        Ok(p) => p.into_owned(),
        Err(e) => {
            log_debug(&format!("Error decoding path: {:?}", e));
            return std::ptr::null_mut();
        }
    };
    
    // 5. Consultar estado REAL via IPC usando worker compartido
    let sync_status = crate::provider::ipc_query_status(&uri).ok();
    
    log_debug(&format!("v3: IPC SyncStatus = {:?}", sync_status));
    
    let mut items: *mut glib_sys::GList = std::ptr::null_mut();
    
    match sync_status {
        Some(SyncStatus::Synced) => {
            // Archivo completamente sincronizado localmente -> Opción: "Liberar espacio"
            log_debug("v3: Showing 'Liberar espacio' (Synced)");
            let item = create_menu_item(
                "gdrivexp::free_space",
                "Liberar espacio",
                "Eliminar copia local, mantener en la nube",
                "weather-few-clouds-symbolic",
            );
            
            let uri_boxed = Box::into_raw(Box::new(uri)) as gpointer;
            connect_activate(item, free_space_callback, uri_boxed);
            items = g_list_append(items, item as gpointer);
        }
        Some(SyncStatus::CloudOnly) => {
            // Archivo solo en la nube -> Opción: "Mantener siempre local"
            log_debug("v3: Showing 'Mantener siempre local' (CloudOnly)");
            let item = create_menu_item(
                "gdrivexp::keep_local",
                "Mantener siempre local",
                "Descargar y mantener copia local",
                "folder-download-symbolic",
            );
            
            let uri_boxed = Box::into_raw(Box::new(uri)) as gpointer;
            connect_activate(item, keep_local_callback, uri_boxed);
            items = g_list_append(items, item as gpointer);
        }
        Some(SyncStatus::LocalOnly) => {
            // Archivo solo local (pendiente de subir) - no mostrar opciones
            log_debug("v3: No menu for LocalOnly");
        }
        _ => {
            // Unknown o error de IPC - no mostrar opciones
            log_debug("v3: No menu (Unknown/Error)");
        }
    }
    
    items
}

// === Helpers ===

// Helper Runtime removed


// IPC Client helper removed (now instantiated per thread)

unsafe fn create_menu_item(
    name: &str,
    label: &str,
    tip: &str,
    icon: &str,
) -> *mut NautilusMenuItem {
    let name_c = str_to_cstring(name);
    let label_c = str_to_cstring(label);
    let tip_c = str_to_cstring(tip);
    let icon_c = str_to_cstring(icon);
    
    nautilus_menu_item_new(
        name_c.as_ptr(),
        label_c.as_ptr(),
        tip_c.as_ptr(),
        icon_c.as_ptr(),
    )
}

unsafe fn connect_activate(
    item: *mut NautilusMenuItem,
    callback: unsafe extern "C" fn(*mut NautilusMenuItem, gpointer),
    user_data: gpointer,
) {
    let signal = str_to_cstring("activate");
    log_debug(&format!("v3: Connecting signal 'activate' to item {:?} with user_data {:?}", item, user_data));
    
    // Transmute callback to generic function pointer
    let cb_ptr: unsafe extern "C" fn() = std::mem::transmute(callback);

    let handler_id = g_signal_connect_data(
        item as gpointer,
        signal.as_ptr(),
        Some(cb_ptr),
        user_data,
        Some(free_user_data),
        0, // G_CONNECT_DEFAULT
    );
    
    log_debug(&format!("v3: g_signal_connect_data returned handler_id: {}", handler_id));
}

unsafe extern "C" fn free_user_data(data: gpointer, _closure: *mut gobject_sys::GClosure) {
    if !data.is_null() {
        let ptr = data as *mut String;
        log_debug(&format!("v3: free_user_data called for {:?}", ptr));
        drop(Box::from_raw(ptr));
    } else {
        log_debug("v3: free_user_data called with NULL");
    }
}

// === Callbacks de Acción ===

unsafe extern "C" fn free_space_callback(
    _item: *mut NautilusMenuItem,
    user_data: gpointer,
) {
    if user_data.is_null() { return; }
    
    let uri = unsafe { &*(user_data as *const String) }.clone();
    
    // Spawn thread to avoid blocking Nautilus UI
    thread::spawn(move || {
        log_debug(&format!("v3: Action Thread Started: Free space for {}", uri));
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
            
        rt.block_on(async {
            let client = IpcClient::new(); // Create new client per thread is cheap
            match client.set_online_only(&uri).await {
                Ok(_) => log_debug("IPC Success: Set Online Only"),
                Err(e) => log_debug(&format!("IPC Error: {:?}", e)),
            }
        });
    });
}

unsafe extern "C" fn keep_local_callback(
    _item: *mut NautilusMenuItem,
    user_data: gpointer,
) {
    if user_data.is_null() { return; }
    
    let uri = unsafe { &*(user_data as *const String) }.clone();
    
    // Spawn thread to avoid blocking Nautilus UI
    thread::spawn(move || {
        log_debug(&format!("v2: Action Thread Started: Keep local for {}", uri));
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
            
        rt.block_on(async {
            let client = IpcClient::new();
            match client.set_local_online(&uri).await {
                Ok(_) => log_debug("IPC Success: Set Local Online"),
                Err(e) => log_debug(&format!("IPC Error: {:?}", e)),
            }
        });
    });
}
