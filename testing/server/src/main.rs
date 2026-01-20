use bevy::app::App;
use bevy::DefaultPlugins;
use bevy::prelude::{ResMut, Startup};
use bevy_egui::EguiPlugin;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use inator::NetworkSide;
use inator::plugins::connection::{ConnectionPlugin, NetworkConnections, ServerConnection};
use inator::ports::PortSideSettings;
use inator::ports::tcp::server::TcpSettingsServer;

fn start_connection(
    mut network_connection: ResMut<NetworkConnections<ServerConnection>>
){
    network_connection.start_connection("Network",PortSideSettings::Server(Box::new(TcpSettingsServer::default())));
}

fn main() {
    App::new().add_plugins((DefaultPlugins,ConnectionPlugin{
        network_side: NetworkSide::Server,
    },EguiPlugin::default(),WorldInspectorPlugin::new())).add_systems(Startup, start_connection).run();
}
