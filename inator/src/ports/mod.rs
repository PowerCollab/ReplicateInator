use crate::ports::tcp::client::ClientTcp;
use crate::ports::tcp::server::ServerTcp;
use crate::ports::tcp::TcpSettings;
#[cfg(target_arch = "wasm32")]
use crate::ports::wasm::client::WasmClient;
#[cfg(target_arch = "wasm32")]
use crate::ports::wasm::WasmSettings;

pub mod tcp;
pub mod wasm;
pub mod websocket;

#[derive(Copy, Clone)]
pub enum PortTypes{
    Tcp,
    Wasm,
    Udp
}

pub enum PortClient{
    #[cfg(not(target_arch = "wasm32"))]
    Tcp(ClientTcp),
    #[cfg(target_arch = "wasm32")]
    Wasm(WasmClient),
    #[cfg(not(target_arch = "wasm32"))]
    Udp
}

pub enum PortServer{
    Tcp(ServerTcp),
    Udp
}

pub enum PortSettings{
    #[cfg(not(target_arch = "wasm32"))]
    Tcp(TcpSettings),
    #[cfg(target_arch = "wasm32")]
    Wasm(WasmSettings)
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for PortServer{
    fn default() -> Self {
        PortServer::Tcp(ServerTcp::default())
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for PortClient{
    fn default() -> Self {
        PortClient::Wasm(WasmClient::default())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for PortClient{
    fn default() -> Self {
        PortClient::Tcp(ClientTcp::default())
    }
}

