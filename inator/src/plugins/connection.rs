use std::collections::HashMap;
use bevy::app::App;
use bevy::prelude::{Plugin, Resource};
use tokio::runtime::Runtime;
use uuid::Uuid;
use crate::NetworkSide;
use crate::plugins::authentication::{AuthenticationMessage, AuthenticationPlugin};
use crate::plugins::client::ClientConnectionPlugin;
use crate::plugins::messaging::MessagingPlugin;
use crate::plugins::server::ServerConnectionPlugin;
use crate::ports::{PortClient, PortServer, PortSettings};
use crate::ports::tcp::client::ClientTcp;
use crate::ports::tcp::server::{ServerTcp};
use crate::ports::tcp::TcpSettings;

#[cfg(target_arch = "wasm32")]
use crate::ports::wasm::client::WasmClient;

#[cfg(target_arch = "wasm32")]
use crate::ports::wasm::WasmSettings;

pub trait ConnectionTrait: Default{
    fn new(connection_name: &'static str, settings: PortSettings) -> Option<Self>;
    fn start_connection(&mut self);
    fn drop_connection(&mut self);
}

pub struct ConnectionPlugin{
    pub network_side: NetworkSide
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default)]
pub struct ServerConnection{
    pub runtime: Option<Runtime>,
    pub connection_name: &'static str,
    pub connected: bool,
    pub ports: HashMap<u32, PortServer>,
    pub main_port: PortServer,
    pub clients_connected: HashMap<Uuid, Vec<u32>>
}
#[derive(Default)]
pub struct ClientConnection{
    #[cfg(not(target_arch = "wasm32"))]
    pub runtime: Option<Runtime>,
    pub connection_name: &'static str,
    pub connected: bool,
    pub ports: HashMap<u32, PortClient>,
    pub main_port: PortClient,
    pub local_uuid: Option<Uuid>,
}

#[derive(Resource)]
pub struct NetworkSideResource(pub NetworkSide);

#[derive(Resource, Default)]
pub struct NetworkConnections<T: ConnectionTrait>(pub HashMap<String, T>);

#[cfg(not(target_arch = "wasm32"))]
impl ConnectionTrait for ServerConnection{
    fn new(connection_name: &'static str, settings: PortSettings) -> Option<Self>
    {
        match settings {
            PortSettings::Tcp(tcp_settings) => {
                match tcp_settings {
                    TcpSettings::Server(tcp_settings_server) => {
                        let server_tcp = ServerTcp::new(tcp_settings_server).with_main_port();

                        Some(ServerConnection{
                            runtime: None,
                            connection_name,
                            connected: false,
                            ports: HashMap::new(),
                            main_port: PortServer::Tcp(server_tcp),
                            clients_connected: HashMap::new(),
                        })
                    },
                    _ => {
                        print!("This settings is not a TcpSettingsServer");
                        None
                    }
                }
            }
        }
    }

    fn start_connection(&mut self) {
        self.connected = true;

        match &mut self.main_port { 
            PortServer::Tcp(server_tcp) => {
                self.runtime = Some(Runtime::new().unwrap());
                server_tcp.connect(&self.runtime);
            }
            _ => {}
        }
    }

    fn drop_connection(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ServerConnection {
    pub fn authenticate_client(&mut self, uuid: Uuid){
        if self.clients_connected.contains_key(&uuid) {
            println!("{} is already connected", uuid);
        }else{
            self.clients_connected.insert(uuid, [0].to_vec());

            let main_port = &mut self.main_port;
            
            match main_port {
                PortServer::Tcp(server_tcp) => {
                    if server_tcp.not_authenticated_listening.contains_key(&uuid){
                        let mut listening_infos = server_tcp.not_authenticated_listening.remove(&uuid).unwrap();

                        listening_infos.listening = true;

                        server_tcp.listening_clients.insert(uuid, listening_infos);
                        server_tcp.start_listening_authenticated_client(&uuid, &self.runtime);

                        match server_tcp.client_authenticated_sender.send(uuid) {
                            Ok(_) => {}
                            Err(_) => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl ConnectionTrait for ClientConnection{
    fn new(connection_name: &'static str, settings: PortSettings) -> Option<Self> {
        match settings {
            PortSettings::Wasm(wasm_settings) => {
                match wasm_settings {
                    WasmSettings::Client(wasm_settings_client) => {
                        let client_wasm = WasmClient::new(wasm_settings_client).with_main_port();

                        Some(ClientConnection{
                            runtime: None,
                            connection_name,
                            connected: false,
                            ports: HashMap::new(),
                            main_port: PortClient::Wasm(client_wasm),
                            local_uuid: None,
                        })
                    },
                    _ => {
                        print!("This settings is not a WasmSettingsClient");
                        None
                    }
                }
            },
            _ => {
                print!("Invalid settings for main port");
                None
            }
        }
    }

    fn start_connection(&mut self) {
        self.connected = true;

        match &mut self.main_port {
            PortClient::Wasm(client_wasm) => {
                client_wasm.connect();
            }
            _ => {}
        }
    }

    fn drop_connection(&mut self) {

    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ConnectionTrait for ClientConnection{
    fn new(connection_name: &'static str, settings: PortSettings) -> Option<Self> {
        match settings {
            PortSettings::Tcp(tcp_settings) => {
                match tcp_settings {
                    TcpSettings::Client(tcp_settings_client) => {
                        let client_tcp = ClientTcp::new(tcp_settings_client).with_main_port() ;

                        Some(ClientConnection{
                            runtime: None,
                            connection_name,
                            connected: false,
                            ports: HashMap::new(),
                            main_port: PortClient::Tcp(client_tcp),
                            local_uuid: None,
                        })
                    },
                    _ => {
                        print!("This settings is not a TcpSettingsClient");
                        None
                    }
                }
            }
        }
    }

    fn start_connection(&mut self) {
        self.connected = true;

        match &mut self.main_port {
            PortClient::Tcp(client_tcp) => {
                self.runtime = Some(Runtime::new().unwrap());
                client_tcp.connect(&self.runtime);
            }
            _ => {}
        }
    }

    fn drop_connection(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl <T:ConnectionTrait> NetworkConnections<T> {
    pub fn start_connection(&mut self, connection_name: &'static str, settings: PortSettings){
        if !self.can_connect(connection_name) { return };

        let connection = T::new(connection_name, settings);

        if let Some(connection) = connection{
            self.0.insert(connection_name.to_string(), connection);
        }
    }
    pub(crate) fn can_connect(&mut self, connection_name: &'static str) -> bool {
        !self.0.contains_key(connection_name)
    }

    pub fn disconnect(&mut self, connection_name: &'static str){
        let connection = self.0.remove(connection_name);

        if let Some(mut connection) = connection{
            connection.drop_connection()
        }
    }
}

impl Plugin for ConnectionPlugin{
    fn build(&self, app: &mut App) {
        app.add_plugins(MessagingPlugin{
            network_side: self.network_side
        });

        app.insert_resource(NetworkSideResource(self.network_side));

        if self.network_side == NetworkSide::Client {
            app.add_plugins(AuthenticationPlugin::<AuthenticationMessage>::new(
                NetworkSide::Client,
            ));
            
            #[cfg(target_arch = "wasm32")] {
                let client_connection: NetworkConnections<ClientConnection> = Default::default();
                app.init_non_send_resource(client_connection);
            }

            #[cfg(not(target_arch = "wasm32"))]
            app.init_resource::<NetworkConnections<ClientConnection>>();

            app.add_plugins(ClientConnectionPlugin);
        }else if self.network_side == NetworkSide::Server {
            #[cfg(not(target_arch = "wasm32"))]
            app.add_plugins(AuthenticationPlugin::<AuthenticationMessage>::new(
                NetworkSide::Server,
            ));
            
            #[cfg(not(target_arch = "wasm32"))]
            app.init_resource::<NetworkConnections<ServerConnection>>();

            #[cfg(not(target_arch = "wasm32"))]
            app.add_plugins(ServerConnectionPlugin);
        }
    }
}