use std::any::Any;
use bevy::app::{App, Plugin};
use bevy::platform::collections::HashMap;

#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::{Commands, Message, PreUpdate, Res, ResMut, Resource, World};
#[cfg(target_arch = "wasm32")]
use bevy::prelude::{Commands, Message, NonSend, NonSendMut, PreUpdate, Resource, World};

use ciborium::{from_reader};
use log::warn;
use erased_serde::Serialize as ErasedSerialize;
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use uuid::Uuid;
use crate::NetworkSide;
use crate::plugins::connection::{ClientConnection, NetworkConnections, NetworkSideResource, ServerConnection};
use crate::ports::{PortTypes};

#[cfg(target_arch = "wasm32")]
type NetResMut<'a, T> = NonSendMut<'a, T>;

#[cfg(target_arch = "wasm32")]
type NetRes<'a, T> = NonSend<'a, T>;

#[cfg(not(target_arch = "wasm32"))]
type NetResMut<'a, T> = ResMut<'a, T>;

#[cfg(not(target_arch = "wasm32"))]
type NetRes<'a, T> = Res<'a, T>;

#[cfg(not(target_arch = "wasm32"))]
pub trait MessageSideTrait: Send + Sync + 'static + ErasedSerialize {}

#[cfg(target_arch = "wasm32")]
pub trait MessageSideTrait: 'static {}

pub trait MessageTrait: MessageSideTrait {
    fn as_authentication(&self) -> bool {
        false
    }
}

pub trait MessageTraitPlugin{
    fn register_message<T: MessageTrait + DeserializeOwned>(&mut self);
}

pub struct MessagingPlugin{
    pub network_side: NetworkSide,
}

#[derive(Serialize,Deserialize)]
pub struct MessageInfos {
    message_id: u32,
    message: Vec<u8>,
}

#[derive(Message)]
pub struct MessageReceivedFromServer<T: MessageTrait>{
    pub message: T,
    pub port_type: PortTypes,
    pub connection_name: &'static str,
    pub port_number: u32
}

#[derive(Message)]
pub struct MessageReceivedFromClient<T: MessageTrait>{
    pub message: T,
    pub port_type: PortTypes,
    pub sender: Option<Uuid>,
    pub connection_name: &'static str,
    pub port_number: u32
}

#[derive(Resource, Default)]
pub struct MessagesRegistry(u32, HashMap<u32, MessageFunctions>);

pub struct MessageFunctions{
    deserialize: fn(&[u8]) -> Box<dyn Any + Send + Sync + 'static>,
    dispatch: fn(world: &mut World, message: Box<dyn Any>, network_side: &NetworkSide, port_type: PortTypes, port_number: u32, connection_name: &'static str, sender: Option<Uuid>),
    is_authentication_message: fn(message: &Box<dyn Any + Send + Sync>) -> bool
}

#[cfg(not(target_arch = "wasm32"))]
impl <T: Send + Sync + 'static + ErasedSerialize> MessageSideTrait for T {

}

impl<'a> serde::Serialize for dyn MessageTrait + 'a {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        erased_serde::serialize(self, serializer)
    }
}

#[cfg(target_arch = "wasm32")]
impl <T: 'static> MessageSideTrait for T {

}

impl Plugin for MessagingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MessagesRegistry>();

        if self.network_side == NetworkSide::Client {
            app.add_systems(PreUpdate, read_messages_client);
        }else{
            app.add_systems(PreUpdate, read_messages_server);
        }
    }
}

impl MessageTraitPlugin for App {
    fn register_message<T: MessageTrait + DeserializeOwned>(&mut self) {
        let network_side = {
            let world = self.world_mut();
            world.get_resource::<NetworkSideResource>()
                .expect("NetworkSideResource missing — add ConnectionPlugin before MessagingPlugin")
                .0
        };

        match network_side {
            NetworkSide::Client => {
                self.add_message::<MessageReceivedFromServer<T>>();
            }
            NetworkSide::Server => {
                self.add_message::<MessageReceivedFromClient<T>>();
            }
        }

        let world = self.world_mut();

        let _ = {
            let mut msg_registry = world
                .get_resource_mut::<MessagesRegistry>()
                .expect("MessagesRegistry not registered; please add MessagingPlugin first");
            let new_value = msg_registry.0 +1;

            msg_registry.0 = new_value;

            msg_registry.1.insert(new_value, MessageFunctions{
                deserialize: deserialize_message::<T>,
                dispatch: dispatch_message::<T>,
                is_authentication_message: is_authentication_message::<T>
            })
        };
    }
}

fn is_authentication_message<T: MessageTrait + DeserializeOwned>(message: &Box<dyn Any + Send + Sync>) -> bool {
    let message_downcast = message.downcast_ref::<T>().expect("Failed to downcast");

    message_downcast.as_authentication()
}

fn deserialize_message<T: MessageTrait + DeserializeOwned>(bytes: &[u8]) -> Box<dyn Any + Send + Sync + 'static> {
    let msg: T = from_reader(bytes).expect("Failed to decode message");

    Box::new(msg)
}

fn dispatch_message<T: MessageTrait + DeserializeOwned>(world: &mut World, message: Box<dyn Any>, network_side: &NetworkSide, port_type: PortTypes, port_number: u32, connection_name: &'static str, sender: Option<Uuid>)  {
    let message_downcast = message.downcast::<T>().expect("Failed to downcast");

    match network_side {
        NetworkSide::Server => {
            world.write_message(MessageReceivedFromClient {
                message: *message_downcast,
                port_type,
                sender,
                connection_name,
                port_number
            });
        }
        NetworkSide::Client => {
            world.write_message(MessageReceivedFromServer {
                message: *message_downcast,
                port_type,
                connection_name,
                port_number
            });
        }
    }
}

pub fn read_messages_client(
    messages_registry: NetRes<MessagesRegistry>,
    mut client_connections: NetResMut<NetworkConnections<ClientConnection>>,
    mut commands: Commands,
){
    for (_,client_connection) in client_connections.0.iter_mut() {
        let main_port = &mut client_connection.main_port;

        match main_port {
            Some(main_port) => {
                let messages_bytes = main_port.check_messages_received();

                for bytes in messages_bytes {
                    let message_infos: Result<MessageInfos, _> = from_reader(bytes.as_slice());

                    match message_infos {
                        Ok(message_infos) => {
                            if let Some(message_functions) = messages_registry.1.get(&message_infos.message_id) {
                                let message = (message_functions.deserialize)(&*message_infos.message);
                                let dispatch = message_functions.dispatch;
                                let connection_name = client_connection.connection_name;

                                commands.queue(move |world: &mut World| {
                                    dispatch(world, message, &NetworkSide::Client, PortTypes::Tcp, 0, connection_name, None);
                                })
                            }
                        }
                        Err(e) => {
                            println!("Error getting message infos: {}", e);
                            continue
                        }
                    }
                }
            },
            None => {
                continue
            }
        }
    }
}

pub fn read_messages_server(
    messages_registry: NetRes<MessagesRegistry>,
    mut server_connections: NetResMut<NetworkConnections<ServerConnection>>,
    mut commands: Commands,
){
    for (_,server_connection) in server_connections.0.iter_mut() {
        let main_port = &mut server_connection.main_port;

        match main_port {
            Some(main_port) => {
                let messages_vector =main_port.check_messages_received();

                for (bytes,sender) in messages_vector {
                    let message_infos: Result<MessageInfos, _> = from_reader(bytes.as_slice());

                    match message_infos {
                        Ok(message_infos) => {
                            if let Some(message_functions) = messages_registry.1.get(&message_infos.message_id) {
                                let message = (message_functions.deserialize)(&*message_infos.message);
                                let ref_message = &message;
                                let is_authentication_message = message_functions.is_authentication_message;

                                if !server_connection.clients_connected.contains_key(&sender) && !is_authentication_message(ref_message) {
                                    warn!("Client sending messages without being authenticated yet");
                                    continue;
                                }

                                let dispatch = message_functions.dispatch;
                                let connection_name = server_connection.connection_name;

                                commands.queue(move |world: &mut World| {
                                    dispatch(world, message, &NetworkSide::Server, PortTypes::Tcp, 0, connection_name, Some(sender));
                                })
                            }
                        }
                        Err(e) => {
                            println!("Error getting message infos: {}", e);
                            continue
                        }
                    }
                }
            },
            None => {
                continue
            }
        }
    }
}


