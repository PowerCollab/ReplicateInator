use std::sync::Arc;
use bevy::app::{App, First, Plugin};

#[cfg(target_arch = "wasm32")]
use bevy::prelude::{IntoScheduleConfigs, NonSendMut};
#[cfg(target_arch = "wasm32")]
use futures_util::stream::{SplitSink, SplitStream};
#[cfg(target_arch = "wasm32")]
use futures_util::StreamExt;
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::{IntoScheduleConfigs, ResMut};
use tokio::io::split;
use tokio::sync::Mutex;
use crate::plugins::connection::{ClientConnection, ConnectionTrait, NetworkConnections};
use crate::ports::{PortClient};
use crate::ports::tcp::client::{ReadType, StreamType, WriteType};

#[cfg(target_arch = "wasm32")]
type NetRes<'a, T> = NonSendMut<'a, T>;

#[cfg(not(target_arch = "wasm32"))]
type NetRes<'a, T> = ResMut<'a, T>;

pub(crate) struct ClientConnectionPlugin;

impl Plugin for ClientConnectionPlugin{
    fn build(&self, app: &mut App) {
        app.add_systems(First, (check_main_port_dropped,start_connections,check_connected_to_main_port,check_listening_to_main_port).chain());

        #[cfg(target_arch = "wasm32")]
        app.add_systems(First,check_messages_queued_main_port.after(check_listening_to_main_port));
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
            PortClient::Tcp(client_tcp) => {
                match client_tcp.connection_down_receiver.try_recv() {
                    Ok(_) => {
                        client_tcp.connected = false;
                        client_tcp.connecting = false;
                        client_tcp.listening = false;
                        client_connection.connected = false;

                        match client_tcp.write_half.take() {
                            None => {}
                            Some(write_half) => { drop(write_half) }
                        };

                        match client_tcp.read_half.take() {
                            None => {}
                            Some(read_half) => { drop(read_half) }
                        };

                        if client_tcp.settings.try_reconnect { continue }

                        disconnect_vector.push(client_connection.connection_name);
                    }
                    Err(_) => {
                        continue;
                    }
                }
            },
            #[cfg(target_arch = "wasm32")]
            PortClient::Wasm(client_wasm) => {
                match client_wasm.connection_down_receiver.try_recv() {
                    Ok(_) => {
                        client_wasm.connected = false;
                        client_wasm.connecting = false;
                        client_wasm.listening = false;
                        client_connection.connected = false;

                        match client_wasm.split_sink.take() {
                            None => {}
                            Some(split_sink) => { drop(split_sink) }
                        };

                        match client_wasm.split_stream.take() {
                            None => {}
                            Some(split_stream) => { drop(split_stream) }
                        };

                        if client_wasm.settings.try_reconnect { continue }

                        disconnect_vector.push(client_connection.connection_name);
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

fn check_connected_to_main_port(
    mut network_connection: NetRes<NetworkConnections<ClientConnection>>
){
    for (_,client_connection) in network_connection.0.iter_mut() {
        let main_port = &mut client_connection.main_port;

        match main_port {
            PortClient::Tcp(client_tcp) => {
                if client_tcp.connected { continue; }

                match client_tcp.connected_to_server_receiver.try_recv() {
                    Ok((stream_type, socket)) => {
                        client_tcp.connected = true;

                        let settings = &client_tcp.settings;

                        match stream_type {
                            StreamType::Stream(stream) => {
                                match stream.set_nodelay(settings.no_delay) {
                                    Ok(_) => {}
                                    _ => {}
                                }

                                let (read_half,write_half) = stream.into_split();

                                client_tcp.write_half = Some(WriteType::Stream(Arc::new(Mutex::new(write_half))));
                                client_tcp.read_half = Some(ReadType::Stream(Arc::new(Mutex::new(read_half))));
                                client_tcp.socket_addr = Some(socket)
                            }
                            StreamType::TlsStream(mut tls_stream) => {
                                match tls_stream.get_mut().0.set_nodelay(settings.no_delay) {
                                    Ok(_) => {}
                                    _ => {}
                                }

                                let (read_half,write_half) = split(tls_stream);

                                client_tcp.write_half = Some(WriteType::TlsStream(Arc::new(Mutex::new(write_half))));
                                client_tcp.read_half = Some(ReadType::TlsStream(Arc::new(Mutex::new(read_half))));
                                client_tcp.socket_addr = Some(socket)
                            }
                        }
                    }
                    Err(_) => {}
                }
            },
            #[cfg(target_arch = "wasm32")]
            PortClient::Wasm(client_wasm) => {
                if client_wasm.connected { continue; }

                match client_wasm.connected_to_server_receiver.try_recv() {
                    Ok(web_socket) => {
                        client_wasm.connected = true;

                        let (sink,stream) = web_socket.split();

                        client_wasm.split_sink = Some(Rc::new(RefCell::new(sink)));
                        client_wasm.split_stream = Some(Rc::new(RefCell::new(stream)));
                    }
                    Err(_) => {}
                }
            }
            _ => {}
        }
    }
}

fn check_listening_to_main_port(
    mut network_connection: NetRes<NetworkConnections<ClientConnection>>
){
    for (_,client_connection) in network_connection.0.iter_mut() {
        let main_port = &mut client_connection.main_port;

        match main_port {
            PortClient::Tcp(client_tcp) => {
                if client_tcp.listening { continue; }

                client_tcp.start_listening_server(&client_connection.runtime);
            },
            #[cfg(target_arch = "wasm32")]
            PortClient::Wasm(client_wasm) => {
                if client_wasm.listening { continue; }

                client_wasm.start_listening_server();
            }
            _ => {}
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn check_messages_queued_main_port(
    mut network_connection: NetRes<NetworkConnections<ClientConnection>>
){
    for (_,client_connection) in network_connection.0.iter_mut() {
        let main_port = &mut client_connection.main_port;

        match main_port {
            PortClient::Wasm(client_wasm) => {
                client_wasm.send_message_queued();
            }
            _ => {

            }
        }
    }
}