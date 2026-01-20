use bevy::DefaultPlugins;
use bevy::prelude::{App, ResMut, Startup};
use bevy_egui::EguiPlugin;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use inator::NetworkSide;
use inator::plugins::connection::{ClientConnection, ConnectionPlugin, NetworkConnections};
use inator::ports::PortSideSettings;
use inator::ports::tcp::client::TcpSettingsClient;

fn start_connection(
    mut network_connection: ResMut<NetworkConnections<ClientConnection>>
) {
    network_connection.start_connection("Network",PortSideSettings::Client(Box::new(TcpSettingsClient::default())));
}

fn main() {
    App::new().add_plugins((DefaultPlugins,ConnectionPlugin{
        network_side: NetworkSide::Client,
    },EguiPlugin::default(),WorldInspectorPlugin::new())).add_systems(Startup,start_connection).run();
}
