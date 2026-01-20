use std::sync::Arc;
use bevy::log::warn;
use ciborium::into_writer;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use rustls::ClientConfig;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{WebSocketStream, Connector, connect_async_tls_with_config, MaybeTlsStream};
use tungstenite::Bytes;
use crate::plugins::connection::ClientConnection;
use crate::plugins::messaging::MessageTrait;
use crate::ports::{ClientPortTrait, ClientSettingsTrait};
use tungstenite::protocol::WebSocketConfig;
use tungstenite::handshake::client::Response;

pub struct WebSocketSettingsClient {
    pub(crate) address: &'static str,
    pub(crate) try_reconnect: bool,
    pub(crate) no_delay: bool,
    pub(crate) hook_web_socket_stream: Option<fn(tcp_stream: WebSocketStream<MaybeTlsStream<TcpStream>>) -> WebSocketStream<MaybeTlsStream<TcpStream>>>,
    pub(crate) client_config: Option<Arc<WebSocketConfig>>,
    pub(crate) tls_config: Option<Arc<ClientConfig>>
}

pub struct ClientWebSocket {
    pub(crate) settings: WebSocketSettingsClient,
    pub(crate) connecting: bool,
    pub(crate) connected: bool,
    pub(crate) listening: bool,
    pub(crate) main_port: bool,

    pub(crate) connection_down_receiver: UnboundedReceiver<()>,
    pub(crate) connection_down_sender: Arc<UnboundedSender<()>>,

    pub(crate) message_received_receiver: UnboundedReceiver<Vec<u8>>,
    pub(crate) message_received_sender: Arc<UnboundedSender<Vec<u8>>>,

    pub(crate) connected_to_server_receiver: UnboundedReceiver<(WebSocketStream<MaybeTlsStream<TcpStream>>,Response)>,
    pub(crate) connected_to_server_sender: Arc<UnboundedSender<(WebSocketStream<MaybeTlsStream<TcpStream>>,Response)>>,

    pub(crate) split_sink: Option<Arc<Mutex<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>,Message>>>>,
    pub(crate) split_stream: Option<Arc<Mutex<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>>>,
}

impl Default for WebSocketSettingsClient {
    fn default() -> Self {
        WebSocketSettingsClient {
            address: "wss://127.0.0.1:8080",
            try_reconnect: true,
            no_delay: true,
            hook_web_socket_stream: None,
            client_config: None,
            tls_config: None
        }
    }
}

impl Default for ClientWebSocket {
    fn default() -> Self {
        let (message_received_sender,message_received_receiver) = unbounded_channel::<Vec<u8>>();
        let (connected_to_server_sender,connected_to_server_receiver) = unbounded_channel::<(WebSocketStream<MaybeTlsStream<TcpStream>>,Response)>();
        let (connection_down_sender,connection_down_receiver) = unbounded_channel();

        ClientWebSocket {
            settings: WebSocketSettingsClient::default(),
            connecting: false,
            connected: false,
            listening: false,
            main_port: false,

            connection_down_receiver,
            connection_down_sender: Arc::new(connection_down_sender),

            message_received_receiver,
            message_received_sender: Arc::new(message_received_sender),

            connected_to_server_receiver,
            connected_to_server_sender: Arc::new(connected_to_server_sender),

            split_sink: None,
            split_stream: None,
        }
    }
}

impl WebSocketSettingsClient {
    pub fn new(address: &'static str, try_reconnect: bool, no_delay: bool) -> Self{
        WebSocketSettingsClient {
            address,
            try_reconnect,
            no_delay,
            hook_web_socket_stream: None,
            client_config: None,
            tls_config: None
        }
    }

    pub fn with_client_config(mut self, client_config: WebSocketConfig) -> Self {
        self.client_config = Some(Arc::new(client_config));

        self
    }

    pub fn with_tls_connector(mut self, client_config: ClientConfig) -> Self {
        self.tls_config = Some(Arc::new(client_config));

        self
    }
}

impl ClientWebSocket {
    fn new(settings: WebSocketSettingsClient) -> Self{
        let (message_received_sender,message_received_receiver) = unbounded_channel::<Vec<u8>>();
        let (connected_to_server_sender,connected_to_server_receiver) = unbounded_channel::<(WebSocketStream<MaybeTlsStream<TcpStream>>,Response)>();
        let (connection_down_sender,connection_down_receiver) = unbounded_channel();

        ClientWebSocket {
            settings,
            connecting: false,
            connected: false,
            listening: false,
            main_port: false,

            connection_down_receiver,
            connection_down_sender: Arc::new(connection_down_sender),

            message_received_receiver,
            message_received_sender: Arc::new(message_received_sender),

            connected_to_server_receiver,
            connected_to_server_sender: Arc::new(connected_to_server_sender),

            split_sink: None,
            split_stream: None
        }
    }
}

impl ClientSettingsTrait for WebSocketSettingsClient {
    fn create_client_port(self: Box<Self>) -> Option<Box<dyn ClientPortTrait>> {
        Some(Box::new(ClientWebSocket::new(*self)))
    }
}

impl ClientPortTrait for ClientWebSocket {
    fn connect(&mut self, client_connection: &mut ClientConnection) {
        if self.connecting {return; }

        let runtime = &client_connection.runtime;

        if let Some(runtime) = runtime {
            let connected_to_server_sender = Arc::clone(&self.connected_to_server_sender);
            let connection_down_sender = Arc::clone(&self.connection_down_sender);
            let settings = &self.settings;
            let no_delay = settings.no_delay;
            let address = settings.address;
            let try_reconnect = settings.try_reconnect;
            let client_config = match &settings.client_config {
                Some(client_config) => Some(Arc::clone(client_config)),
                None => None,
            };
            let tls_config = match &settings.tls_config {
                Some(client_config) => Some(Arc::clone(client_config)),
                None => None,
            };

            self.connecting = true;

            runtime.spawn(async move {
                let result = if let Some(tls_config) = tls_config {
                    match client_config {
                        Some(client_config) => {
                            connect_async_tls_with_config(address, Some(*client_config), no_delay, Some(Connector::Rustls(tls_config)))
                        }
                        None => {
                            connect_async_tls_with_config(address, None, no_delay, Some(Connector::Rustls(tls_config)))
                        }
                    }
                } else {
                    match client_config {
                        Some(client_config) => {
                            connect_async_tls_with_config(address, Some(*client_config), no_delay, None)
                        }
                        None => {
                            connect_async_tls_with_config(address, None, no_delay, None)
                        }
                    }
                };

                match result.await {
                    Ok((web_socket_stream,response)) => {
                        println!("Connected to server");
                        connected_to_server_sender.send((web_socket_stream,response)).expect("Could not send connected_to_server");
                    },
                    Err(_) => {
                        if try_reconnect {
                            print!("Failed to connect to server, trying again");
                        }else{
                            print!("Failed to connect to server, closing the connection");
                            connection_down_sender.send(()).expect("Error to send connection down client");
                            return;
                        }
                    }
                }
            });
        }
    }

    fn as_main_port(&mut self) -> bool {
        self.main_port = true;
        true
    }

    fn check_messages_received(&mut self) -> Vec<Vec<u8>> {
        let mut messages_byes = Vec::new();

        loop {
            match self.message_received_receiver.try_recv() {
                Ok(bytes) => {
                    messages_byes.push(bytes);
                },
                Err(_) =>{
                    break
                }
            }
        }

        messages_byes
    }

    fn check_port_dropped(&mut self) -> (bool, bool) {
        match self.connection_down_receiver.try_recv() {
            Ok(_) => {
                self.connected = false;
                self.connecting = false;
                self.listening = false;

                match self.split_sink.take() {
                    None => {}
                    Some(split_sink) => { drop(split_sink) }
                };

                match self.split_stream.take() {
                    None => {}
                    Some(split_stream) => { drop(split_stream) }
                };

                (true, self.settings.try_reconnect)
            }
            Err(_) => {
                (false,false)
            }
        }
    }

    fn check_connected_to_server(&mut self) -> bool {
        if self.connected { return false; }

        match self.connected_to_server_receiver.try_recv() {
            Ok((mut web_socket_stream,_)) => {
                self.connected = true;

                let settings = &self.settings;

                web_socket_stream = match settings.hook_web_socket_stream {
                    Some(hook_web_socket_stream) => {
                        hook_web_socket_stream(web_socket_stream)
                    },
                    None => {
                        web_socket_stream
                    }
                };

                let (split_sink,split_stream) = web_socket_stream.split();

                self.split_sink = Some(Arc::new(Mutex::new(split_sink)));
                self.split_stream = Some(Arc::new(Mutex::new(split_stream)));
                
                true
            }
            Err(_) => {
                false
            }
        }
    }

    fn start_listening_to_server(&mut self, client_connection: &mut ClientConnection) {
        if self.listening { return; }
        
        let runtime = &client_connection.runtime;

        if let Some(runtime) = runtime {
            let split_stream = match &self.split_stream {
                Some(split_stream) => {
                    Arc::clone(split_stream)
                }
                None => {
                    return;
                }
            };
            let message_received_sender = Arc::clone(&self.message_received_sender);
            let connection_down_sender = Arc::clone(&self.connection_down_sender);
            
            self.listening = true;
            
            runtime.spawn(async move {
                let mut guard = split_stream.lock().await;

                loop {
                    match guard.next().await {
                        Some(result) => {
                            match result {
                                Ok(Message::Binary(bytes)) => {
                                    message_received_sender.send(Vec::from(bytes)).expect("Failed to send message");
                                }
                                Err(_) => {
                                    connection_down_sender.send(()).expect("Failed to send disconnected from server");
                                },
                                _ => {
                                    continue
                                }
                            }
                        }
                        None => {
                            
                        }
                    }
                }
            });
        }
    }

    fn send_message(&mut self, message: Box<&dyn MessageTrait>, client_connection: &mut ClientConnection) {
        let runtime = &client_connection.runtime;

        if let Some(runtime) = runtime {
            let split_sink = match &self.split_sink {
                Some(split_sink) => {
                    Arc::clone(split_sink)
                }
                None => {
                    return;
                }
            };

            let mut buf = Vec::new();

            match into_writer(&message,&mut buf) {
                Ok(_) => {}
                Err(_) => {
                    warn!("Error to serialize message");
                    return;
                }
            };
            
            runtime.spawn(async move {
                let mut guard = split_sink.lock().await;

                if let Err(e) = guard.send(Message::Binary(Bytes::from(buf))).await {
                    log::error!("Error sending message: {:?}", e);
                }
            });
        }
    }
}