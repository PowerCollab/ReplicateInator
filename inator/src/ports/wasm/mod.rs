use crate::ports::wasm::client::WasmSettingsClient;
pub mod client;

pub enum WasmSettings{
    Client(WasmSettingsClient),
}