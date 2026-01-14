use std::marker::PhantomData;
use crate::plugins::messaging::{read_messages_client, read_messages_server, MessageReceivedFromClient, MessageReceivedFromServer, MessageTrait, MessageTraitPlugin};
use bevy::app::App;
use bevy::log::warn;
#[cfg(target_arch = "wasm32")]
use bevy::prelude::{Commands, IntoScheduleConfigs, MessageMutator, MessageReader, Plugin, PreUpdate, NonSendMut};
#[cfg(not(target_arch = "wasm32"))]
use bevy::prelude::{Commands, IntoScheduleConfigs, MessageMutator, MessageReader, Plugin, PreUpdate, ResMut};
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use uuid::Uuid;
use pro_macros::ConnectionMessage;
use crate::NetworkSide;
#[cfg(target_arch = "wasm32")]
use crate::plugins::connection::{ClientConnection, NetworkConnections};
#[cfg(not(target_arch = "wasm32"))]
use crate::plugins::connection::{ClientConnection, NetworkConnections, ServerConnection};
use crate::ports::PortServer;

#[cfg(target_arch = "wasm32")]
type ConnMut<T> = NonSendMut<T>;

#[cfg(not(target_arch = "wasm32"))]
type ConnMut<'a, T> = ResMut<'a, T>;

#[cfg(not(target_arch = "wasm32"))]
pub trait AuthenticationTrait: Serialize + DeserializeOwned {
    fn can_authenticate(&mut self, server_connection: &ServerConnection, commands: &mut Commands) -> (bool,&str) ;
    fn authenticate(&mut self, server_connection: &ServerConnection, commands: &mut Commands) -> (bool,&str);
}

#[cfg(target_arch = "wasm32")]
pub struct AuthenticationPlugin {
    pub(crate) network_side: NetworkSide,
}

#[cfg(not(target_arch = "wasm32"))]
pub struct AuthenticationPlugin<T: AuthenticationTrait + MessageTrait> {
    pub(crate) network_side: NetworkSide,
    _marker: PhantomData<T>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Serialize,Deserialize,ConnectionMessage)]
#[connection_message(authentication = true)]
pub struct AuthenticationMessage{
    username: String,
    password: String
}

#[derive(Serialize,Deserialize,ConnectionMessage)]
pub struct AuthenticatedUuid(pub Uuid);

#[cfg(not(target_arch = "wasm32"))]
impl<T> AuthenticationPlugin<T>
where
    T: AuthenticationTrait + MessageTrait,
{
    pub fn new(network_side: NetworkSide) -> Self {
        Self {
            network_side,
            _marker: PhantomData,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl AuthenticationTrait for AuthenticationMessage {
    fn can_authenticate(&mut self, _: &ServerConnection, _: &mut Commands) -> (bool,&str) {
        (true, "Success")
    }

    fn authenticate(&mut self, server_connection: &ServerConnection, commands: &mut Commands) -> (bool,&str) {
        let (can,message) = self.can_authenticate(server_connection, commands);

        (can,message)
    }
}

#[cfg(target_arch = "wasm32")]
impl Plugin for AuthenticationPlugin {
    fn build(&self, app: &mut App) {
        app.register_message::<AuthenticatedUuid>();

        match self.network_side {
            NetworkSide::Client => {
                app.add_systems(PreUpdate, authentication_message_client.after(read_messages_client));
            },
            _ => {}
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T: AuthenticationTrait + MessageTrait> Plugin for AuthenticationPlugin<T> {
    fn build(&self, app: &mut App) {
        app.register_message::<T>();
        app.register_message::<AuthenticatedUuid>();

        match self.network_side {
            NetworkSide::Server => {
                app.add_systems(PreUpdate, authentication_message_server::<T>.after(read_messages_server));
            }
            NetworkSide::Client => {
                app.add_systems(PreUpdate, authentication_message_client.after(read_messages_client));
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn authentication_message_server<T: AuthenticationTrait + MessageTrait>(
    mut server_connections: ResMut<NetworkConnections<ServerConnection>>,
    mut message_received_from_client: MessageMutator<MessageReceivedFromClient<T>>,
    mut commands: Commands,
){
    for ev in message_received_from_client.read() {
        let message = &mut ev.message;
        let uuid = &ev.sender.unwrap();

        let server_connection = server_connections.0.get_mut(ev.connection_name);

        if let Some(server_connection) = server_connection{
            let (can,message) = message.authenticate(&server_connection, &mut commands);

            if can {
                server_connection.authenticate_client(*uuid);

                match &mut server_connection.main_port {
                    PortServer::Tcp(server_tcp) => {
                        server_tcp.send_message::<AuthenticatedUuid>(&AuthenticatedUuid(*uuid),&uuid,&server_connection.runtime);
                    }
                    _ => {}
                }

            }else {
                warn!("Failed to authenticate connection, reason: {}", message);
                continue;
            }
        }
    }
}

pub fn authentication_message_client(
    mut client_connections: ConnMut<NetworkConnections<ClientConnection>>,
    mut message_received_from_server: MessageReader<MessageReceivedFromServer<AuthenticatedUuid>>,
){
    for ev in message_received_from_server.read() {
        let message = &ev.message;

        if let Some(client_connection) = client_connections.0.get_mut(ev.connection_name) {
            client_connection.local_uuid = Some(message.0);
        }
    }
}
