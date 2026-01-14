use crate::ports::tcp::client::TcpSettingsClient;
use crate::ports::tcp::server::TcpSettingsServer;

pub mod client;
pub mod server;
pub mod reader_writer;
pub mod read_writter_client;

pub enum TcpSettings{
    Server(TcpSettingsServer),
    Client(TcpSettingsClient),
}