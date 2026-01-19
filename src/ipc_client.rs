//! Cliente IPC para comunicación con el daemon de G-DriveXP

use serde::{Deserialize, Serialize};
use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// Cliente IPC que se comunica con el daemon vía Unix Socket
pub struct IpcClient {
    socket_path: std::path::PathBuf,
}

impl IpcClient {
    /// Crea un nuevo cliente IPC
    pub fn new() -> Self {
        let uid = unsafe { libc::getuid() };
        let socket_path = std::path::PathBuf::from(format!("/run/user/{}/gdrivexp.sock", uid));
        
        Self { socket_path }
    }
    
    /// Consulta el estado de sincronización de un archivo
    pub async fn get_file_status(&self, path: &str) -> io::Result<crate::SyncStatus> {
        let request = IpcRequest::GetFileStatus {
            path: path.to_string(),
        };
        
        match self.send_request(request).await? {
            IpcResponse::FileStatus(status) => Ok(status),  // CAMBIADO de Status a FileStatus
            _ => Ok(crate::SyncStatus::Unknown),
        }
    }

    
    /// Cambia archivo a online_only
    pub async fn set_online_only(&self, path: &str) -> io::Result<bool> {
        let request = IpcRequest::SetOnlineOnly {
            path: path.to_string(),
        };
        
        match self.send_request(request).await? {
            IpcResponse::Success => Ok(true),  // CAMBIADO de Ok a Success
            _ => Ok(false),
        }
    }
    
    /// Cambia archivo a local_online
    pub async fn set_local_online(&self, path: &str) -> io::Result<bool> {
        let request = IpcRequest::SetLocalOnline {
            path: path.to_string(),
        };
        
        match self.send_request(request).await? {
            IpcResponse::Success => Ok(true),  // CAMBIADO de Ok a Success
            _ => Ok(false),
        }
    }

    
    /// Helper genérico para enviar requests
    async fn send_request(&self, request: IpcRequest) -> io::Result<IpcResponse> {
        // Conectar al socket
        let mut stream = match UnixStream::connect(&self.socket_path).await {
            Ok(s) => s,
            Err(_) => {
                // Si no podemos conectar, el daemon no está corriendo
                return Ok(IpcResponse::Error {
                    message: "Daemon no disponible".to_string(),
                });
            }
        };
        
        // Serializar request
        let request_bytes = bincode::serialize(&request)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        
        // Enviar longitud + request
        let len = (request_bytes.len() as u32).to_be_bytes();
        stream.write_all(&len).await?;
        stream.write_all(&request_bytes).await?;
        
        // Leer longitud de respuesta
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let response_len = u32::from_be_bytes(len_buf) as usize;
        
        if response_len > 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Respuesta IPC demasiado grande",
            ));
        }
        
        // Leer respuesta
        let mut response_buf = vec![0u8; response_len];
        stream.read_exact(&mut response_buf).await?;
        
        // Deserializar respuesta
        let response: IpcResponse = bincode::deserialize(&response_buf)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        
        Ok(response)
    }
}

/// Request IPC (debe coincidir EXACTAMENTE con src/ipc/mod.rs del daemon)
#[derive(Debug, Serialize, Deserialize)]
enum IpcRequest {
    GetFileStatus { path: String },
    Ping,
    SetOnlineOnly { path: String },
    SetLocalOnline { path: String },
    GetFileAvailability { path: String },
}

/// Respuesta IPC (debe coincidir EXACTAMENTE con src/ipc/mod.rs del daemon)
#[derive(Debug, Serialize, Deserialize)]
enum IpcResponse {
    FileStatus(crate::SyncStatus),
    Pong,
    Availability(crate::FileAvailability),
    Success,  // ¡CAMBIADO de Ok a Success!
    Error { message: String },
}

