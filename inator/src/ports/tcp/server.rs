use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use bevy::log::warn;
use bevy::platform::collections::HashMap;
use ciborium::into_writer;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::io::{split, AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::{Mutex, Semaphore};
use tokio::sync::mpsc::error::TryRecvError;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;
use uuid::Uuid;
use crate::NetworkSide;
use crate::plugins::{BytesOptions, OrderOptions};
use crate::plugins::messaging::MessageTrait;
use crate::ports::tcp::reader_writer::{read_from_settings, read_from_settings_not_owned, read_value_to_usize, value_from_number, write_from_settings, write_from_settings_not_owned};

pub enum StreamType{
    Stream(TcpStream),
    TlsStream(TlsStream<TcpStream>),
}

pub enum ReadWriteType {
    Stream((Arc<Mutex<OwnedReadHalf>>, Arc<Mutex<OwnedWriteHalf>>)),
    TlsStream((Arc<Mutex<ReadHalf<TlsStream<TcpStream>>>>, Arc<Mutex<WriteHalf<TlsStream<TcpStream>>>>)),
}

pub enum ReadType{
    Stream(Arc<Mutex<OwnedReadHalf>>),
    TlsStream(Arc<Mutex<ReadHalf<TlsStream<TcpStream>>>>),
}

pub enum WriteType{
    Stream(Arc<Mutex<OwnedWriteHalf>>),
    TlsStream(Arc<Mutex<WriteHalf<TlsStream<TcpStream>>>>),
}

pub struct TcpSettingsServer{
    pub(crate) address: IpAddr,
    pub(crate) port: u16,
    pub(crate) bytes: BytesOptions,
    pub(crate) order: OrderOptions,
    pub(crate) max_connections: usize,
    pub(crate) recuse_when_full: bool,
    pub(crate) auto_reconnect: bool,
    pub(crate) no_delay: bool,
    pub(crate) tls: bool,
    pub(crate) tls_server_config: Option<Arc<ServerConfig>>
}

#[allow(dead_code)]
pub struct ListeningInfos{
    pub(crate) read_write_type: ReadWriteType,
    pub(crate) socket_addr: SocketAddr,
    pub(crate) listening: bool
}

pub struct ServerTcp {
    pub(crate) settings: TcpSettingsServer,
    pub(crate) connecting: bool,
    pub(crate) connected: bool,
    pub(crate) main_port: bool,
    pub(crate) accepting_connections: bool,
    pub(crate) tcp_listener: Option<Arc<TcpListener>>,

    pub(crate) port_connected_receiver: UnboundedReceiver<TcpListener>,
    pub(crate) port_connected_sender: Arc<UnboundedSender<TcpListener>>,

    pub(crate) connection_down_receiver: UnboundedReceiver<()>,
    pub(crate) connection_down_sender: Arc<UnboundedSender<()>>,
    
    pub(crate) client_disconnected_receiver: UnboundedReceiver<(Uuid,bool)>,
    pub(crate) client_disconnected_sender: Arc<UnboundedSender<(Uuid,bool)>>,

    pub(crate) message_received_receiver: UnboundedReceiver<(Vec<u8>,Uuid)>,
    pub(crate) message_received_sender: Arc<UnboundedSender<(Vec<u8>,Uuid)>>,

    pub(crate) client_connected_receiver: UnboundedReceiver<(StreamType,SocketAddr)>,
    pub(crate) client_connected_sender: Arc<UnboundedSender<(StreamType,SocketAddr)>>,

    pub(crate) client_authenticated_receiver: Arc<Mutex<UnboundedReceiver<Uuid>>>,
    pub(crate) client_authenticated_sender: UnboundedSender<Uuid>,

    pub(crate) listening_clients: HashMap<Uuid,ListeningInfos>,
    pub(crate) not_authenticated_listening: HashMap<Uuid,ListeningInfos>
}

impl Default for TcpSettingsServer {
    fn default() -> Self {
        TcpSettingsServer{
            address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 8080,
            bytes: BytesOptions::U32,
            order: OrderOptions::LittleEndian,
            max_connections: 0,
            recuse_when_full: false,
            auto_reconnect: true,
            no_delay: true,
            tls: false,
            tls_server_config: None,
        }
    }
}

impl Default for ServerTcp {
    fn default() -> Self {
        let (port_connected_sender,port_connected_receiver) = unbounded_channel::<TcpListener>();
        let (connection_down_sender,connection_down_receiver) = unbounded_channel();
        let (message_received_sender,message_received_receiver) = unbounded_channel::<(Vec<u8>,Uuid)>();
        let (client_connected_sender,client_connected_receiver) = unbounded_channel::<(StreamType,SocketAddr)>();
        let (client_authenticated_sender,client_authenticated_receiver) = unbounded_channel::<Uuid>();
        let (client_disconnected_sender,client_disconnected_receiver) = unbounded_channel::<(Uuid,bool)>();

        ServerTcp{
            settings: TcpSettingsServer::default(),
            connecting: false,
            connected: false,
            main_port: false,
            accepting_connections: false,
            tcp_listener: None,

            port_connected_receiver,
            port_connected_sender: Arc::new(port_connected_sender),

            connection_down_receiver,
            connection_down_sender: Arc::new(connection_down_sender),

            client_disconnected_receiver,
            client_disconnected_sender: Arc::new(client_disconnected_sender),

            message_received_receiver,
            message_received_sender: Arc::new(message_received_sender),

            client_connected_receiver,
            client_connected_sender: Arc::new(client_connected_sender),

            client_authenticated_receiver: Arc::new(Mutex::new(client_authenticated_receiver)),
            client_authenticated_sender,

            listening_clients: HashMap::new(),
            not_authenticated_listening: HashMap::new(),
        }
    }
}

impl TcpSettingsServer {
    pub fn new(address: IpAddr, port: u16, bytes: BytesOptions, order: OrderOptions, max_connections: usize, recuse_when_full: bool, auto_reconnect: bool, no_delay: bool) -> Self {
        TcpSettingsServer{
            address,
            port,
            bytes,
            order,
            max_connections,
            recuse_when_full,
            auto_reconnect,
            no_delay,
            tls: false,
            tls_server_config: None,
        }
    }

    pub fn with_tls(mut self, server_config: ServerConfig) -> Self {
        self.tls = true;
        self.tls_server_config = Some(Arc::new(server_config));

        self
    }
}

impl StreamType {
    pub fn split(self) -> ReadWriteType {
        match self {
            StreamType::Stream(stream) => {
                let (r, w) = stream.into_split();
                ReadWriteType::Stream((Arc::new(Mutex::new(r)), Arc::new(Mutex::new(w))))
            },
            StreamType::TlsStream(tls_stream) => {
                let (r, w) = split(tls_stream);
                ReadWriteType::TlsStream((Arc::new(Mutex::new(r)), Arc::new(Mutex::new(w))))
            }
        }
    }
}

impl ReadWriteType {
    pub fn get_write(&mut self) -> WriteType {
        match self {
            ReadWriteType::Stream((_, write)) => {
                WriteType::Stream(Arc::clone(&write))
            },
            ReadWriteType::TlsStream((_, write)) => {
                WriteType::TlsStream(Arc::clone(&write))
            }
        }
    }

    pub fn get_read(&mut self) -> ReadType {
        match self {
            ReadWriteType::Stream((read, _)) => {
                ReadType::Stream(Arc::clone(&read))
            },
            ReadWriteType::TlsStream((read, _)) => {
                ReadType::TlsStream(Arc::clone(&read))
            }
        }
    }
}

impl ReadType {
    pub fn read(&self, target: Uuid, bytes_options: BytesOptions, order_options: OrderOptions, runtime: &Runtime, message_received_sender: Arc<UnboundedSender<(Vec<u8>,Uuid)>>, client_disconnected_sender: Arc<UnboundedSender<(Uuid,bool)>>, client_authenticated_receiver: Option<Arc<Mutex<UnboundedReceiver<Uuid>>>>) {
        match self {
            ReadType::Stream(read) => {
                let read = Arc::clone(read);

                runtime.spawn(async move {
                    let mut guard = read.lock().await;

                    loop {
                        if let Some(client_authenticated_receiver) = &client_authenticated_receiver {
                            let mut guard_authenticated_receiver = client_authenticated_receiver.lock().await;

                            loop {
                                match guard_authenticated_receiver.try_recv() {
                                    Ok(uuid) => {
                                        if uuid == target {
                                            return;
                                        }
                                    }
                                    Err(error) => {
                                        if error == TryRecvError::Empty {
                                            break
                                        }else{
                                            return;
                                        }
                                    }
                                }
                            }
                        }

                        match read_from_settings(&mut guard, &bytes_options, &order_options).await {
                            Ok(read_value) => {
                                let size = read_value_to_usize(read_value);
                                let mut buf = vec![0u8; size];

                                match guard.read_exact(&mut buf).await {
                                    Ok(_) => {
                                        message_received_sender.send((buf,target)).expect("Failed to send message");
                                    }
                                    Err(_) => {
                                        client_disconnected_sender.send((target, true)).expect("Failed to send client disconnected");
                                        return;
                                    }
                                }
                            }
                            Err(_) => {
                                client_disconnected_sender.send((target, true)).expect("Failed to send client disconnected");
                                return;
                            }
                        }
                    }
                });
            },
            ReadType::TlsStream(read) => {
                let read = Arc::clone(read);

                runtime.spawn(async move {
                    let mut guard = read.lock().await;

                    loop {
                        if let Some(client_authenticated_receiver) = &client_authenticated_receiver {
                            let mut guard_authenticated_receiver = client_authenticated_receiver.lock().await;

                            loop {
                                match guard_authenticated_receiver.try_recv() {
                                    Ok(uuid) => {
                                        if uuid == target {
                                            return;
                                        }
                                    }
                                    Err(error) => {
                                        if error == TryRecvError::Empty {
                                            break
                                        }else{
                                            return;
                                        }
                                    }
                                }
                            }
                        }

                        match read_from_settings_not_owned(&mut guard, &bytes_options, &order_options).await {
                            Ok(read_value) => {
                                let size = read_value_to_usize(read_value);
                                let mut buf = vec![0u8; size];

                                match guard.read_exact(&mut buf).await {
                                    Ok(_) => {
                                        message_received_sender.send((buf,target)).expect("Failed to send message");
                                    }
                                    Err(_) => {
                                        client_disconnected_sender.send((target, true)).expect("Failed to send client disconnected");
                                        return;
                                    }
                                }
                            }
                            Err(_) => {
                                client_disconnected_sender.send((target, true)).expect("Failed to send client disconnected");
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
    pub fn write(&self, buf: Vec<u8>, bytes_options: BytesOptions, order_options: OrderOptions, runtime: &Runtime) {
        match self {
            WriteType::Stream(write) => {
                let message_size = buf.len();
                let write = Arc::clone(write);

                runtime.spawn(async move {
                    let mut guard = write.lock().await;

                    let size_value = value_from_number(message_size as f64, bytes_options);

                    if let Err(e) = write_from_settings(&mut guard, &size_value, &order_options).await {
                        eprintln!("Failed to send size: {:?}", e);
                        eprintln!("From {:?}", NetworkSide::Server);
                        return;
                    }

                    if let Err(e) = guard.write_all(&buf).await {
                        eprintln!("Failed to send message: {:?}", e);
                        eprintln!("From {:?}", NetworkSide::Server);
                        return;
                    }
                });
            },
            WriteType::TlsStream(write) => {
                let message_size = buf.len();
                let write = Arc::clone(write);

                runtime.spawn(async move {
                    let mut guard = write.lock().await;

                    let size_value = value_from_number(message_size as f64, bytes_options);

                    if let Err(e) = write_from_settings_not_owned(&mut guard, &size_value, &order_options).await {
                        eprintln!("Failed to send size: {:?}", e);
                        eprintln!("From {:?}", NetworkSide::Server);
                        return;
                    }

                    if let Err(e) = guard.write_all(&buf).await {
                        eprintln!("Failed to send message: {:?}", e);
                        eprintln!("From {:?}", NetworkSide::Server);
                        return;
                    }
                });
            }
        }
    }
}

impl ServerTcp {
    pub fn new(settings: TcpSettingsServer) -> ServerTcp{
        let (port_connected_sender,port_connected_receiver) = unbounded_channel::<TcpListener>();
        let (connection_down_sender,connection_down_receiver) = unbounded_channel();
        let (message_received_sender,message_received_receiver) = unbounded_channel::<(Vec<u8>,Uuid)>();
        let (client_connected_sender,client_connected_receiver) = unbounded_channel::<(StreamType,SocketAddr)>();
        let (client_authenticated_sender,client_authenticated_receiver) = unbounded_channel::<Uuid>();
        let (client_disconnected_sender,client_disconnected_receiver) = unbounded_channel::<(Uuid,bool)>();
        
        ServerTcp{
            settings,
            connecting: false,
            connected: false,
            main_port: false,
            accepting_connections: false,
            tcp_listener: None,

            port_connected_receiver,
            port_connected_sender: Arc::new(port_connected_sender),

            connection_down_receiver,
            connection_down_sender: Arc::new(connection_down_sender),

            client_disconnected_receiver,
            client_disconnected_sender: Arc::new(client_disconnected_sender),

            message_received_receiver,
            message_received_sender: Arc::new(message_received_sender),

            client_connected_receiver,
            client_connected_sender: Arc::new(client_connected_sender),

            client_authenticated_receiver: Arc::new(Mutex::new(client_authenticated_receiver)),
            client_authenticated_sender,

            listening_clients: HashMap::new(),
            not_authenticated_listening: HashMap::new(),
        }
    }
    
    pub fn set_auto_reconnect(mut self, auto_reconnect: bool) -> Self{
        self.settings.auto_reconnect = auto_reconnect;
        
        self
    }

    pub fn with_main_port(mut self) -> Self {
        self.main_port = true;
        self
    }
    
    pub fn connect(&mut self, runtime: &Option<Runtime>){
        if let Some(runtime) = runtime {
            if self.connecting {return;}

            self.connecting = true;

            let settings = &self.settings;
            let address = (settings.address, settings.port);
            let port_connected_sender = Arc::clone(&self.port_connected_sender);

            runtime.spawn(async move {
                loop {
                    let tcp_listener_future = TcpListener::bind(&address);

                    match tcp_listener_future.await {
                        Ok(tcp_listener) => {
                            print!("Server successful connected");
                            port_connected_sender.send(tcp_listener).expect("Failed to send TCP listener");
                            break;
                        }
                        Err(_) => {
                            print!("Server connection bind failed, trying again");
                        }
                    }
                }
            });
        }
    }

    pub fn send_message<T: MessageTrait + Serialize + DeserializeOwned>(&mut self, message: &T, target: &Uuid, runtime: &Option<Runtime>){
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
            let listening_infos = match self.listening_clients.get_mut(target) {
                Some(listening_infos) => listening_infos,
                None => {
                    warn!("Client from uuid {} not found ", target);
                    return;
                }
            };
            
            let write_half = &listening_infos.read_write_type.get_write();

            write_half.write(buf, bytes_options, order_options, &runtime);
        }
    }

    pub fn start_listening_authenticated_client(&mut self, target: &Uuid, runtime: &Option<Runtime>){
        if let Some(runtime) = runtime {
            let message_received_sender = Arc::clone(&self.message_received_sender);
            let client_disconnected_sender = Arc::clone(&self.client_disconnected_sender);

            let listening_infos = match self.listening_clients.get_mut(target) {
                Some(listening_infos) => listening_infos,
                None => {
                    warn!("Client from uuid {} not found ", target);
                    return;
                }
            };

            let read_type = &listening_infos.read_write_type.get_read();
            let bytes_options = self.settings.bytes;
            let order_options = self.settings.order;
            let target = *target;

            read_type.read(target,bytes_options,order_options,runtime,message_received_sender,client_disconnected_sender, None);
        }
    }

    pub fn start_listening_not_authenticated_client(&mut self, target: &Uuid, runtime: &Option<Runtime>){
        if let Some(runtime) = runtime {
            let client_authenticated_receiver = Arc::clone(&self.client_authenticated_receiver);
            let message_received_sender = Arc::clone(&self.message_received_sender);
            let client_disconnected_sender = Arc::clone(&self.client_disconnected_sender);
            
            let listening_infos = match self.not_authenticated_listening.get_mut(target) {
                Some(listening_infos) => listening_infos,
                None => {
                    warn!("Client from uuid {} not found ", target);
                    return;
                }
            };
            let read_type = &listening_infos.read_write_type.get_read();
            let bytes_options = self.settings.bytes;
            let order_options = self.settings.order;
            let target = *target;

            read_type.read(target,bytes_options,order_options,runtime,message_received_sender,client_disconnected_sender, Some(client_authenticated_receiver));
        }
    }

    pub fn start_accepting_connections(&mut self, runtime: &Option<Runtime>){
        if let Some(runtime) = runtime {
            if self.accepting_connections || !self.connected {return;}

            self.accepting_connections = true;

            let connection_down_sender = Arc::clone(&self.connection_down_sender);
            let client_connected_sender = Arc::clone(&self.client_connected_sender);

            let max_connections = self.settings.max_connections;
            let recuse_when_full = self.settings.recuse_when_full;
            let tcp_listener = match &self.tcp_listener {
                Some(tcp_listener) => Arc::clone(&tcp_listener),
                None => { print!("Cant listening for clients because the listener is dropped"); self.accepting_connections = false; return }
            };
            let settings = &self.settings;

            let mut tls_acceptor: Option<TlsAcceptor> = None;

            if let Some(tls_config) = &settings.tls_server_config {
                tls_acceptor = Some(TlsAcceptor::from(Arc::clone(tls_config)));
            }

            runtime.spawn(async move {
                let semaphore: Option<Semaphore> = if max_connections > 0 { Some(Semaphore::new(max_connections)) } else { None };
                let acceptor = if tls_acceptor.is_some() { Some(tls_acceptor.unwrap()) } else { None };

                loop {
                    match tcp_listener.accept().await {
                        Ok((stream, addr)) => {
                            match &semaphore {
                                Some(semaphore) => {
                                    if recuse_when_full {
                                        match semaphore.try_acquire() {
                                            Ok(permit) => {
                                                drop(permit);

                                                if let Some(acceptor) = &acceptor{
                                                   match acceptor.accept(stream).await {
                                                       Ok(tls_stream) => {
                                                           client_connected_sender.send((StreamType::TlsStream(tls_stream), addr)).expect("Failed to send new client connected");
                                                       }
                                                       Err(_) => {
                                                           println!("Error on accepting connection via tls");
                                                       }
                                                   }
                                                }else{
                                                    client_connected_sender.send((StreamType::Stream(stream), addr)).expect("Failed to send new client connected");
                                                }
                                            }
                                            Err(_) => {
                                                println!("Connection is full, rejecting connection to {}", addr);

                                                drop(stream);
                                            }
                                        }

                                    }else{
                                        match semaphore.acquire().await {
                                            Ok(permit) => {
                                                drop(permit);

                                                if let Some(acceptor) = &acceptor{
                                                    match acceptor.accept(stream).await {
                                                        Ok(tls_stream) => {
                                                            client_connected_sender.send((StreamType::TlsStream(tls_stream), addr)).expect("Failed to send new client connected");
                                                        }
                                                        Err(_) => {
                                                            println!("Error on accepting connection via tls");
                                                        }
                                                    }
                                                }else{
                                                    client_connected_sender.send((StreamType::Stream(stream), addr)).expect("Failed to send new client connected");
                                                }
                                            }
                                            Err(_) => {
                                                drop(stream);
                                            }
                                        }
                                    }
                                }
                                None => {
                                    if let Some(acceptor) = &acceptor{
                                        match acceptor.accept(stream).await {
                                            Ok(tls_stream) => {
                                                client_connected_sender.send((StreamType::TlsStream(tls_stream), addr)).expect("Failed to send new client connected");
                                            }
                                            Err(_) => {
                                                println!("Error on accepting connection via tls");
                                            }
                                        }
                                    }else{
                                        client_connected_sender.send((StreamType::Stream(stream), addr)).expect("Failed to send new client connected");
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            connection_down_sender.send(()).expect("Failed to send connection down");
                            break
                        }
                    }
                }
            });
        }
    }
}