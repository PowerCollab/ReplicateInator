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

        /*
        match main_port {
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
        } */
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

        /*
        match main_port {
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
        }*/
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