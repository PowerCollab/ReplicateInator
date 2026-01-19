use bevy::app::{App, First, Plugin};

#[cfg(target_arch = "wasm32")]
use bevy::prelude::{IntoScheduleConfigs, NonSendMut};

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::{IntoScheduleConfigs, ResMut};
use crate::plugins::connection::{ClientConnection, ConnectionTrait, NetworkConnections};

#[cfg(target_arch = "wasm32")]
type NetRes<'a, T> = NonSendMut<'a, T>;

#[cfg(not(target_arch = "wasm32"))]
type NetRes<'a, T> = ResMut<'a, T>;

pub(crate) struct ClientConnectionPlugin;

impl Plugin for ClientConnectionPlugin{
    fn build(&self, app: &mut App) {
        app.add_systems(First, (check_main_port_dropped,start_connections,check_connected_to_main_port,check_listening_to_main_port).chain());
    }
}

fn start_connections(
    mut network_connection: NetRes<NetworkConnections<ClientConnection>>
){
    for (_,client_connection) in network_connection.0.iter_mut() {
        if !client_connection.connected {
            client_connection.start_connection();
        }
    }
}

fn check_main_port_dropped(
    mut network_connection: NetRes<NetworkConnections<ClientConnection>>
){
    let mut disconnect_vector = Vec::new();

    for (_,client_connection) in network_connection.0.iter_mut() {
        let main_port = &mut client_connection.main_port;

        match main_port {
            Some(main_port) => {
                let (dropped,can_reconnect) = main_port.check_port_dropped();

                if dropped {
                    client_connection.connected = false;

                    if can_reconnect { continue; }

                    disconnect_vector.push(client_connection.connection_name);
                }
            }
            None => {
                continue
            }
        }
    }

    for connection_name in disconnect_vector {
        network_connection.disconnect(connection_name);
    }
}

fn check_connected_to_main_port(
    mut network_connection: NetRes<NetworkConnections<ClientConnection>>
){
    for (_,client_connection) in network_connection.0.iter_mut() {
        let main_port = &mut client_connection.main_port;

        match main_port {
            Some(main_port) => {
                main_port.check_connected_to_server();
            },
            None => {
                continue
            }
        }
    }
}

fn check_listening_to_main_port(
    mut network_connection: NetRes<NetworkConnections<ClientConnection>>
){
    for (_,client_connection) in network_connection.0.iter_mut() {
        if let Some(mut main_port) = client_connection.main_port.take() {
            main_port.start_listening_to_server(client_connection);

            client_connection.main_port = Some(main_port);
        }
    }
}