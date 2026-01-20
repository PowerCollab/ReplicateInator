use uuid::Uuid;
use crate::plugins::connection::{ClientConnection, ServerConnection};
use crate::plugins::messaging::MessageTrait;

pub mod tcp;
pub mod websocket_wasm;
pub mod websocket;
pub mod webtransport;
pub mod webrtc;
pub mod udp;
pub mod quic;

#[cfg(not(target_arch = "wasm32"))]
pub trait PortTraits: Send + Sync {}

#[cfg(target_arch = "wasm32")]
pub trait PortTraits {}

pub trait ServerPortTrait: PortTraits{
    fn connect(&mut self, server_connection: &mut ServerConnection);
    fn authenticate_client(&mut self, uuid: Uuid, server_connection: &mut ServerConnection);
    fn as_main_port(&mut self) -> bool;
    fn check_messages_received(&mut self) -> Vec<(Vec<u8>, Uuid)>;
    fn check_port_dropped(&mut self) -> (bool,bool);
    fn check_port_connected(&mut self) -> bool;
    fn start_accepting_connections(&mut self, server_connection: &mut ServerConnection);
    fn check_clients_diconnected(&mut self) -> Vec<Uuid>;
    fn check_clients_connected(&mut self) -> Vec<Uuid>;
    fn check_not_authenticated_clients(&mut self, server_connection: &mut ServerConnection);
    fn start_listening_not_authenticated_client(&mut self, target: Uuid, server_connection: &mut ServerConnection);
    fn start_listening_authenticated_client(&mut self, target: Uuid, server_connection: &mut ServerConnection);
    fn send_message(&mut self, message: Box<&dyn MessageTrait>, target: &Uuid, server_connection: &mut ServerConnection);
}

pub trait ClientPortTrait: PortTraits{
    fn connect(&mut self, client_connection: &mut ClientConnection);
    fn as_main_port(&mut self) -> bool;
    fn check_messages_received(&mut self) -> Vec<Vec<u8>>;
    fn check_port_dropped(&mut self) -> (bool,bool);
    fn check_connected_to_server(&mut self) -> bool;
    fn start_listening_to_server(&mut self, client_connection: &mut ClientConnection);
    fn send_message(&mut self, message: Box<&dyn MessageTrait>, client_connection: &mut ClientConnection);
}

pub trait ServerSettingsTrait{
    fn create_server_port(self: Box<Self>) -> Option<Box<dyn ServerPortTrait>>;
}

pub trait ClientSettingsTrait{
    fn create_client_port(self: Box<Self>) -> Option<Box<dyn ClientPortTrait>>;
}

#[derive(Copy, Clone)]
pub enum PortTypes{
    Tcp,
    Wasm,
    Udp
}

pub enum PortSideSettings {
    Client(Box<dyn ClientSettingsTrait>),
    Server(Box<dyn ServerSettingsTrait>),
}

#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + Sync> PortTraits for T {}

#[cfg(target_arch = "wasm32")]
impl<T> PortTraits for T {}

