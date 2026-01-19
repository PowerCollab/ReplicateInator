use std::collections::HashMap;
use bevy::app::App;
use bevy::prelude::{Plugin, Resource};
use log::warn;
use tokio::runtime::Runtime;
use uuid::Uuid;
use crate::NetworkSide;
use crate::plugins::authentication::{AuthenticationMessage, AuthenticationPlugin};
use crate::plugins::client::ClientConnectionPlugin;
use crate::plugins::messaging::MessagingPlugin;
use crate::plugins::server::ServerConnectionPlugin;
use crate::ports::{ClientPortTrait, PortSideSettings, ServerPortTrait};

pub trait ConnectionTrait: Default{
    fn new(connection_name: &'static str, settings: PortSideSettings) -> Option<Self>;
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
    pub ports: HashMap<u32, Box<dyn ServerPortTrait>>,
    pub main_port: Option<Box<dyn ServerPortTrait>>,
    pub clients_connected: HashMap<Uuid, Vec<u32>>
}

#[derive(Default)]
pub struct ClientConnection{
    #[cfg(not(target_arch = "wasm32"))]
    pub runtime: Option<Runtime>,
    pub connection_name: &'static str,
    pub connected: bool,
    pub ports: HashMap<u32, Box<dyn ClientPortTrait>>,
    pub main_port: Option<Box<dyn ClientPortTrait>>,
    pub local_uuid: Option<Uuid>,
}

#[derive(Resource)]
pub struct NetworkSideResource(pub NetworkSide);

#[derive(Resource, Default)]
pub struct NetworkConnections<T: ConnectionTrait>(pub HashMap<String, T>);
#[cfg(not(target_arch = "wasm32"))]
impl ConnectionTrait for ServerConnection{
    fn new(connection_name: &'static str, settings: PortSideSettings) -> Option<Self>
    {
        match settings {
            PortSideSettings::Server(mut settings) => {
                let server_port_option = settings.create_server_port();

                if let Some(server_port) = server_port_option {
                    Some(ServerConnection{
                        runtime: None,
                        connection_name,
                        connected: false,
                        ports: HashMap::new(),
                        main_port: Some(server_port),
                        clients_connected: HashMap::new(),
                    })
                }else{
                    warn!("This settings is invalid");
                    None
                }
            },
            PortSideSettings::Client(_) => {
                warn!("Client settings not allowed on server");
                None
            },
        }
    }

    fn start_connection(&mut self) {
        self.connected = true;

        #[cfg(not(target_arch = "wasm32"))]
        if self.runtime.is_none() {
            self.runtime = Some(Runtime::new().unwrap());
        }

        if let Some(mut main_port) = self.main_port.take() {
            main_port.connect(self);

            self.main_port = Some(main_port);
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
    pub fn authenticate_client(&mut self, uuid: Uuid, port_number: u32){
        if self.clients_connected.contains_key(&uuid) {
            println!("{} is already connected", uuid);
        }else{
            self.clients_connected.insert(uuid, [0].to_vec());

            if port_number == 0 {
                if let Some(mut main_port) = self.main_port.take() {
                    main_port.authenticate_client(uuid, self);
                    
                    self.main_port = Some(main_port);
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl ConnectionTrait for ClientConnection{
    fn new(connection_name: &'static str, settings: PortSideSettings) -> Option<Self> {
        match settings {
            PortSideSettings::Client(settings) => {
                let client_port = settings.create_client_port();

                if let Some(client_port) = client_port {
                    Some(ClientConnection{
                        runtime: None,
                        connection_name,
                        connected: false,
                        ports: HashMap::new(),
                        main_port: Some(client_port),
                        local_uuid: None,
                    })
                }else {
                    warn!("This settings is invalid");
                    None
                }
            }
            PortSideSettings::Server(_) => {
                warn!("Server settings not allowed on server");
                None
            }
        }
    }

    fn start_connection(&mut self) {
        self.connected = true;

        #[cfg(not(target_arch = "wasm32"))]
        if self.runtime.is_none() {
            self.runtime = Some(Runtime::new().unwrap());
        }

        if let Some(mut main_port) = self.main_port.take() {
            main_port.connect(self);

            self.main_port = Some(main_port);
        }
    }

    fn drop_connection(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

impl <T:ConnectionTrait> NetworkConnections<T> {
    pub fn start_connection(&mut self, connection_name: &'static str, settings: PortSideSettings){
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