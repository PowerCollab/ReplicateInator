use bevy::app::App;
use bevy::prelude::{First, IntoScheduleConfigs, Plugin, ResMut};
use crate::plugins::connection::{ConnectionTrait, NetworkConnections, ServerConnection};

pub(crate) struct ServerConnectionPlugin;

impl Plugin for ServerConnectionPlugin{
    fn build(&self, app: &mut App) {
        app.add_systems(First, (check_main_port_dropped,start_connections,receive_main_port_listener,start_accepting_connections_main_port,check_client_disconnected_from_port,check_client_connected_to_port,start_listening_not_authenticated).chain());
    }
}

fn start_connections(
    mut network_connection: ResMut<NetworkConnections<ServerConnection>>
){
    for (_,server_connection) in network_connection.0.iter_mut() {
        if !server_connection.connected{
            server_connection.start_connection();
        }
    }
}

fn receive_main_port_listener(
    mut network_connection: ResMut<NetworkConnections<ServerConnection>>
){
    for (_,server_connection) in network_connection.0.iter_mut() {
        let main_port = &mut server_connection.main_port;
        
        match main_port {
            Some(main_port) => {
                main_port.check_port_connected();
            }
            None => {
                continue
            }
        }
    }
}

fn start_accepting_connections_main_port(
    mut network_connection: ResMut<NetworkConnections<ServerConnection>>
){
    for (_,server_connection) in network_connection.0.iter_mut() {
        
        if let Some(mut main_port) = server_connection.main_port.take() {
            main_port.start_accepting_connections(server_connection);
            
            server_connection.main_port = Some(main_port);
        }
    }
}

fn check_main_port_dropped(
    mut network_connection: ResMut<NetworkConnections<ServerConnection>>
){
    let mut disconnect_vector = Vec::new();
    
    for (_,server_connection) in network_connection.0.iter_mut() {
        let main_port = &mut server_connection.main_port;
        
        match main_port { 
            Some(main_port) => {
                let (dropped,can_reconnect) = main_port.check_port_dropped();
                
                if dropped {
                    server_connection.connected = false;
                    
                    if can_reconnect { continue }

                    disconnect_vector.push(server_connection.connection_name);
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

fn check_client_disconnected_from_port(
    mut network_connection: ResMut<NetworkConnections<ServerConnection>>
){
    for (_,server_connection) in network_connection.0.iter_mut() {
        let main_port = &mut server_connection.main_port;
        
        match main_port { 
            Some(main_port) => {
                main_port.check_clients_diconnected();
            },
            None => {
                continue
            }
        }
    }
}

fn check_client_connected_to_port(
    mut network_connection: ResMut<NetworkConnections<ServerConnection>>
){
    for (_,server_connection) in network_connection.0.iter_mut() {
        let main_port = &mut server_connection.main_port;
        
        match main_port { 
            Some(main_port) => {
                main_port.check_client_connected();
            }
            None => {
                continue
            }
        }
    }
}

fn start_listening_not_authenticated(
    mut network_connection: ResMut<NetworkConnections<ServerConnection>>
){
    for (_,server_connection) in network_connection.0.iter_mut() {
        if let Some(mut main_port) = server_connection.main_port.take() {
            main_port.check_not_authenticated_clients(server_connection);
            
            server_connection.main_port = Some(main_port);
        }
    }
}