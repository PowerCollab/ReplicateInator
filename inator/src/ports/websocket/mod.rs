use crate::ports::websocket::server::WebSocketSettingsServer;

pub mod server;
pub mod client;

pub enum WebsocketSettings{
    Server(WebSocketSettingsServer),
    Client
}