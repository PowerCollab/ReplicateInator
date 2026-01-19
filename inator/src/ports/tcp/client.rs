use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use bevy::log::warn;
use ciborium::into_writer;
use tokio::io::{split, AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex;
use tokio_rustls::rustls::{ClientConfig};
use tokio_rustls::client::TlsStream;
use tokio_rustls::{TlsConnector};
use crate::NetworkSide;
use crate::plugins::{BytesOptions, OrderOptions};
use crate::plugins::messaging::MessageTrait;
use crate::ports::tcp::read_writter_client::{read_from_settings, read_from_settings_not_owned, read_value_to_usize, value_from_number, write_from_settings, write_from_settings_not_owned};
use rustls::pki_types::{ServerName};
use crate::plugins::connection::{ClientConnection};
use crate::ports::{ClientPortTrait, ClientSettingsTrait};

pub enum StreamType{
    Stream(TcpStream),
    TlsStream(TlsStream<TcpStream>),
}

pub enum ReadType{
    Stream(Arc<Mutex<OwnedReadHalf>>),
    TlsStream(Arc<Mutex<ReadHalf<TlsStream<TcpStream>>>>),
}

pub enum WriteType{
    Stream(Arc<Mutex<OwnedWriteHalf>>),
    TlsStream(Arc<Mutex<WriteHalf<TlsStream<TcpStream>>>>),
}

#[derive(Clone)]
pub enum DomainName{
    Empty,
    Name(String),
    Address(IpAddr)
}

pub struct TcpSettingsClient{
    pub(crate) address: IpAddr,
    pub(crate) port: u16,
    pub(crate) bytes: BytesOptions,
    pub(crate) order: OrderOptions,
    pub(crate) try_reconnect: bool,
    pub(crate) no_delay: bool,
    pub(crate) tls: bool,
    pub(crate) tls_client_config: Option<Arc<ClientConfig>>,
    pub(crate) tls_domain_name: DomainName,
    pub(crate) hook_stream: Option<fn(tcp_stream: TcpStream) -> TcpStream>
}

pub struct ClientTcp {
    pub(crate) settings: TcpSettingsClient,
    pub(crate) connecting: bool,
    pub(crate) connected: bool,
    pub(crate) listening: bool,
    pub(crate) main_port: bool,
 
    pub(crate) connection_down_receiver: UnboundedReceiver<()>,
    pub(crate) connection_down_sender: Arc<UnboundedSender<()>>,
    
    pub(crate) message_received_receiver: UnboundedReceiver<Vec<u8>>,
    pub(crate) message_received_sender: Arc<UnboundedSender<Vec<u8>>>,

    pub(crate) connected_to_server_receiver: UnboundedReceiver<(StreamType,SocketAddr)>,
    pub(crate) connected_to_server_sender: Arc<UnboundedSender<(StreamType,SocketAddr)>>,
    
    pub(crate) write_half: Option<WriteType>,
    pub(crate) read_half: Option<ReadType>,
    pub(crate) socket_addr: Option<SocketAddr>
}

impl ReadType {
    pub fn read(&self, bytes_options: BytesOptions, order_options: OrderOptions, message_received_sender: Arc<UnboundedSender<Vec<u8>>>, connection_down_sender: Arc<UnboundedSender<()>>, runtime: &Runtime){
        match self {
            ReadType::Stream(read) => {
                let read_half = Arc::clone(read);

                runtime.spawn(async move {
                    let mut guard = read_half.lock().await;

                    loop {
                        match read_from_settings(&mut guard, &bytes_options, &order_options).await {
                            Ok(read_value) => {
                                let size = read_value_to_usize(read_value);
                                let mut buf = vec![0u8; size];

                                match guard.read_exact(&mut buf).await {
                                    Ok(_) => {
                                        message_received_sender.send(buf).expect("Failed to send message");
                                    }
                                    Err(_) => {
                                        connection_down_sender.send(()).expect("Failed to send disconnected from server");
                                        return;
                                    }
                                }
                            }
                            Err(_) => {
                                connection_down_sender.send(()).expect("Failed to send disconnected from server");
                                return;
                            }
                        }
                    }
                });
            },

            ReadType::TlsStream(read) => {
                let read_half = Arc::clone(read);

                runtime.spawn(async move {
                    let mut guard = read_half.lock().await;

                    loop {
                        match read_from_settings_not_owned(&mut guard, &bytes_options, &order_options).await {
                            Ok(read_value) => {
                                let size = read_value_to_usize(read_value);
                                let mut buf = vec![0u8; size];

                                match guard.read_exact(&mut buf).await {
                                    Ok(_) => {
                                        message_received_sender.send(buf).expect("Failed to send message");
                                    }
                                    Err(_) => {
                                        connection_down_sender.send(()).expect("Failed to send disconnected from server");
                                        return;
                                    }
                                }
                            }
                            Err(_) => {
                                connection_down_sender.send(()).expect("Failed to send disconnected from server");
                                return;
                            }
                        }
                    }
                });
            }
        }
    }
}

impl WriteType {
    pub fn write(&self, buf: Vec<u8>, bytes_options: BytesOptions, order_options: OrderOptions, runtime: &Runtime){
        match self {
            WriteType::Stream(write) => {
                let message_size = buf.len();
                let write_half = Arc::clone(write);

                runtime.spawn(async move {
                    let mut guard = write_half.lock().await;

                    let size_value = value_from_number(message_size as f64, bytes_options);

                    if let Err(e) = write_from_settings(&mut guard, &size_value, &order_options).await {
                        eprintln!("Failed to send size: {:?}", e);
                        eprintln!("From {:?}", NetworkSide::Client);
                        return;
                    }

                    if let Err(e) = guard.write_all(&buf).await {
                        eprintln!("Failed to send message: {:?}", e);
                        eprintln!("From {:?}", NetworkSide::Client);
                        return;
                    }
                });
            },

            WriteType::TlsStream(write) => {
                let message_size = buf.len();
                let write_half = Arc::clone(write);

                runtime.spawn(async move {
                    let mut guard = write_half.lock().await;

                    let size_value = value_from_number(message_size as f64, bytes_options);

                    if let Err(e) = write_from_settings_not_owned(&mut guard, &size_value, &order_options).await {
                        eprintln!("Failed to send size: {:?}", e);
                        eprintln!("From {:?}", NetworkSide::Client);
                        return;
                    }

                    if let Err(e) = guard.write_all(&buf).await {
                        eprintln!("Failed to send message: {:?}", e);
                        eprintln!("From {:?}", NetworkSide::Client);
                        return;
                    }
                });
            }
        }
    }
}

impl ClientSettingsTrait for TcpSettingsClient {
    fn create_client_port(self: Box<TcpSettingsClient>) -> Option<Box<dyn ClientPortTrait>> {
        Some(Box::new(ClientTcp::new(*self)))
    }
}

impl Default for TcpSettingsClient{
    fn default() -> Self {
        TcpSettingsClient{
            address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 8080,
            bytes: BytesOptions::U32,
            order: OrderOptions::LittleEndian,
            try_reconnect: true,
            no_delay: true,
            tls: false,
            tls_client_config: None,
            tls_domain_name: DomainName::Empty,
            hook_stream: None
        }
    }
}

impl ClientPortTrait for ClientTcp{

    fn connect(&mut self, client_connection: &mut ClientConnection) {
        let runtime = &client_connection.runtime;

        if let Some(runtime) = runtime {
            if self.connecting {return;}

            self.connecting = true;

            let settings = &self.settings;
            let hook_stream = settings.hook_stream;
            let address = (settings.address, settings.port);
            let try_reconnect = settings.try_reconnect;
            let connected_to_server_sender = Arc::clone(&self.connected_to_server_sender);
            let connection_down_sender = Arc::clone(&self.connection_down_sender);
            let tls_domain_name = &settings.tls_domain_name;
            let server_name = match tls_domain_name {
                DomainName::Empty => {
                    match  ServerName::try_from("localhost"){
                        Ok(server_name) => {server_name}
                        Err(_) => {self.connecting = false; return}
                    }
                }
                DomainName::Name(name) => {
                    match ServerName::try_from(name.clone()){
                        Ok(server_name) => {server_name}
                        Err(_) => {self.connecting = false; return}
                    }
                }
                DomainName::Address(address) => {
                    match ServerName::try_from(*address){
                        Ok(server_name) => {server_name}
                        Err(_) => {self.connecting = false; return}
                    }
                }
            };


            let mut tls_connector: Option<TlsConnector> = None;

            if let Some(tls_config) = &settings.tls_client_config {
                tls_connector = Some(TlsConnector::from(Arc::clone(tls_config)));
            }

            runtime.spawn(async move {
                loop {
                    let tcp_stream_future = TcpStream::connect(&address);

                    match tcp_stream_future.await {
                        Ok(mut tcp_stream) => {

                            tcp_stream = match hook_stream {
                                Some(hook_stream) => {
                                    hook_stream(tcp_stream)
                                }
                                None => {
                                    tcp_stream
                                }
                            };

                            let socket_address = tcp_stream.peer_addr().unwrap();
                            if let Some(tls_connector) = &tls_connector {
                                match tls_connector.connect(server_name,tcp_stream).await {
                                    Ok(tls_stream) => {
                                        connected_to_server_sender.send((StreamType::TlsStream(tls_stream), socket_address)).expect("Error to send connected to server");
                                    }
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
                            }else{
                                connected_to_server_sender.send((StreamType::Stream(tcp_stream), socket_address)).expect("Error to send connected to server");
                            }

                            return;
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
                }
            });
        }
    }

    fn as_main_port(&mut self) -> bool {
        self.main_port = true;

        true
    }

    fn check_messages_received(&mut self) -> Option<Vec<u8>> {
        match self.message_received_receiver.try_recv() {
            Ok(bytes) => {
                Some(bytes)
            },
            Err(_) =>{
                None
            }
        }
    }

    fn check_port_dropped(&mut self) -> (bool, bool) {
        match self.connection_down_receiver.try_recv() {
            Ok(_) => {
                self.connected = false;
                self.connecting = false;
                self.listening = false;

                match self.write_half.take() {
                    None => {}
                    Some(write_half) => { drop(write_half) }
                };

                match self.read_half.take() {
                    None => {}
                    Some(read_half) => { drop(read_half) }
                };

                (true, self.settings.try_reconnect)
            }
            Err(_) => {
                (false,false)
            }
        }
    }

    fn check_connected_to_server(&mut self) {
        if self.connected { return; }

        match self.connected_to_server_receiver.try_recv() {
            Ok((stream_type, socket)) => {
                self.connected = true;

                let settings = &self.settings;

                match stream_type {
                    StreamType::Stream(stream) => {
                        match stream.set_nodelay(settings.no_delay) {
                            Ok(_) => {}
                            _ => {}
                        }

                        let (read_half,write_half) = stream.into_split();

                        self.write_half = Some(WriteType::Stream(Arc::new(Mutex::new(write_half))));
                        self.read_half = Some(ReadType::Stream(Arc::new(Mutex::new(read_half))));
                        self.socket_addr = Some(socket)
                    }
                    StreamType::TlsStream(mut tls_stream) => {
                        match tls_stream.get_mut().0.set_nodelay(settings.no_delay) {
                            Ok(_) => {}
                            _ => {}
                        }

                        let (read_half,write_half) = split(tls_stream);

                        self.write_half = Some(WriteType::TlsStream(Arc::new(Mutex::new(write_half))));
                        self.read_half = Some(ReadType::TlsStream(Arc::new(Mutex::new(read_half))));
                        self.socket_addr = Some(socket)
                    }
                }
            }
            Err(_) => {}
        }
    }

    fn start_listening_to_server(&mut self, client_connection: &mut ClientConnection) {
        let runtime = &client_connection.runtime;

        if let Some(runtime) = runtime {
            self.listening = true;

            let message_received_sender = Arc::clone(&self.message_received_sender);
            let connection_down_sender = Arc::clone(&self.connection_down_sender);
            let read_type = &self.read_half;
            let bytes_options = self.settings.bytes;
            let order_options = self.settings.order;

            if let Some(read_type) = &read_type {
                read_type.read(bytes_options, order_options, message_received_sender, connection_down_sender, runtime);
            }
        }
    }

    fn send_message(&mut self, message: Box<&dyn MessageTrait>, client_connection: &mut ClientConnection) {
        let runtime = &client_connection.runtime;

        if let Some(runtime) = runtime {
            let mut buf = Vec::new();

            match into_writer(&message,&mut buf) {
                Ok(_) => {}
                Err(_) => {
                    warn!("Error to serialize message");
                    return;
                }
            };

            let bytes_options = self.settings.bytes;
            let order_options = self.settings.order;
            let write_type = &self.write_half;

            if let Some(write_type) = &write_type {
                write_type.write(buf, bytes_options, order_options, runtime);
            }
        }
    }
}

impl Default for ClientTcp{
    fn default() -> Self {
        let (message_received_sender,message_received_receiver) = unbounded_channel::<Vec<u8>>();
        let (connected_to_server_sender,connected_to_server_receiver) = unbounded_channel::<(StreamType,SocketAddr)>();
        let (connection_down_sender,connection_down_receiver) = unbounded_channel();
        
        ClientTcp {
            settings: TcpSettingsClient::default(),
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
            
            write_half: None,
            read_half: None,
            socket_addr: None
        }
    }
}

impl ClientTcp {
    pub fn new(settings: TcpSettingsClient) -> ClientTcp{
        let (message_received_sender,message_received_receiver) = unbounded_channel::<Vec<u8>>();
        let (connected_to_server_sender,connected_to_server_receiver) = unbounded_channel::<(StreamType,SocketAddr)>();
        let (connection_down_sender,connection_down_receiver) = unbounded_channel();
        
        ClientTcp{
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
            
            write_half: None,
            read_half: None,
            socket_addr: None
        }
    }
}

impl TcpSettingsClient{
    pub fn new(address: IpAddr, port: u16, bytes: BytesOptions, order: OrderOptions, try_reconnect: bool, no_delay: bool) -> Self {
        TcpSettingsClient{
            address,
            port,
            bytes,
            order,
            try_reconnect,
            no_delay,
            tls: false,
            tls_client_config: None,
            tls_domain_name: DomainName::Empty,
            hook_stream: None
        }
    }

    pub fn with_tls(mut self, client_config: ClientConfig, domain_name: DomainName) -> Self {
        self.tls = true;
        self.tls_client_config = Some(Arc::new(client_config));
        self.tls_domain_name = domain_name;

        self
    }
}