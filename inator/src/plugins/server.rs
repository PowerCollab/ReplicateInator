use std::sync::Arc;
use bevy::app::App;
use bevy::prelude::{First, IntoScheduleConfigs, Plugin, ResMut};
use tokio::io::split;
use tokio::sync::Mutex;
use uuid::Uuid;
use crate::plugins::connection::{ConnectionTrait, NetworkConnections, ServerConnection};
use crate::ports::PortServer;
use crate::ports::tcp::server::{ListeningInfos, ReadWriteType, StreamType};

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
            PortServer::Tcp(server_tcp) => {
                if server_tcp.connected { continue; }

                match server_tcp.port_connected_receiver.try_recv() {
                    Ok(tcp_listener) => {
                        server_tcp.connected = true;
                        server_tcp.tcp_listener = Some(Arc::new(tcp_listener));
                    }
                    Err(_) => {
                        continue;
                    }
                }
            }
            _ => {}
        }
    }
}

fn start_accepting_connections_main_port(
    mut network_connection: ResMut<NetworkConnections<ServerConnection>>
){
    for (_,server_connection) in network_connection.0.iter_mut() {
        let main_port = &mut server_connection.main_port;
        
        match main_port {
            PortServer::Tcp(server_tcp) => {
                if server_tcp.accepting_connections { continue; }

                server_tcp.start_accepting_connections(&server_connection.runtime);
            }
            _ => {}
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
            PortServer::Tcp(server_tcp) => {
                match server_tcp.connection_down_receiver.try_recv() {
                    Ok(_) => {
                        server_tcp.connected = false;
                        server_tcp.connecting = false;
                        server_tcp.accepting_connections = false;
                        server_connection.connected = false;

                        if server_tcp.settings.auto_reconnect {
                            continue
                        }

                        match server_tcp.tcp_listener.take() {
                            Some(tcp_listener) => {
                                drop(tcp_listener);
                            },
                            None => {}
                        }

                        disconnect_vector.push(server_connection.connection_name);
                    }
                    Err(_) => {
                        continue;
                    }
                }
            }
            _ => {}
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
            PortServer::Tcp(server_tcp) => {
                match server_tcp.client_disconnected_receiver.try_recv() {
                    Ok((uuid,authenticated)) => {
                        let listening_infos = if authenticated {server_tcp.listening_clients.get_mut(&uuid)} else {server_tcp.not_authenticated_listening.get_mut(&uuid)};

                        if let Some(_) = listening_infos{
                            if authenticated {
                                server_tcp.listening_clients.remove(&uuid);
                            }else{
                                server_tcp.not_authenticated_listening.remove(&uuid);
                            }
                        }
                    }
                    Err(_) => {

                    }
                }
            }
            _ => {}
        }
    }
}

fn check_client_connected_to_port(
    mut network_connection: ResMut<NetworkConnections<ServerConnection>>
){
    for (_,server_connection) in network_connection.0.iter_mut() {
        let main_port = &mut server_connection.main_port;
        
        match main_port { 
            PortServer::Tcp(server_tcp) => {
                match server_tcp.client_connected_receiver.try_recv() {
                    Ok((stream_type,socket_addr)) => {
                        let new_uuid = Uuid::new_v4();
                        let settings = &server_tcp.settings;
                        
                        match stream_type {
                            StreamType::Stream(stream) => {
                                match stream.set_nodelay(settings.no_delay) {
                                    Ok(_) => {}
                                    _ => {}
                                }

                                let (read_half,write_half) = stream.into_split();

                                let listening_infos = ListeningInfos{
                                    read_write_type: ReadWriteType::Stream((Arc::new(Mutex::new(read_half)), Arc::new(Mutex::new(write_half)))),
                                    socket_addr,
                                    listening: false
                                };

                                server_tcp.not_authenticated_listening.insert(new_uuid, listening_infos);
                            }
                            StreamType::TlsStream(mut tls_stream) => {
                                match tls_stream.get_mut().0.set_nodelay(settings.no_delay) {
                                    Ok(_) => {}
                                    _ => {}
                                }

                                let (read_half,write_half) = split(tls_stream);

                                let listening_infos = ListeningInfos{
                                    read_write_type: ReadWriteType::TlsStream((Arc::new(Mutex::new(read_half)), Arc::new(Mutex::new(write_half)))),
                                    socket_addr,
                                    listening: false
                                };

                                server_tcp.not_authenticated_listening.insert(new_uuid, listening_infos);
                            }
                        }
                    }
                    Err(_) => {

                    }
                }
            }
            _ => {}
        }
    }
}

fn start_listening_not_authenticated(
    mut network_connection: ResMut<NetworkConnections<ServerConnection>>
){
    for (_,server_connection) in network_connection.0.iter_mut() {
        let main_port = &mut server_connection.main_port;

        let mut keys_to_start = Vec::new();
        
        match main_port { 
            PortServer::Tcp(server_tcp) => {
                for (key,infos) in server_tcp.not_authenticated_listening.iter_mut(){
                    if infos.listening { continue; }

                    infos.listening = true;

                    keys_to_start.push(*key);
                }

                for key in keys_to_start {
                    server_tcp.start_listening_not_authenticated_client(
                        &key,
                        &server_connection.runtime
                    );
                }
            },
            _ => {}
        }
    }
}