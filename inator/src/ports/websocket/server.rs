use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use bevy::log::warn;
use bevy::platform::collections::HashMap;
use ciborium::into_writer;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::{Mutex, Semaphore};
use tokio::sync::mpsc::error::TryRecvError;
use tokio_tungstenite::{accept_async, WebSocketStream};
use tokio_tungstenite::tungstenite::{Bytes, Message};
use uuid::Uuid;
use crate::plugins::connection::ServerConnection;
use crate::plugins::messaging::MessageTrait;
use crate::ports::{ServerPortTrait, ServerSettingsTrait};

pub struct WebSocketSettingsServer{
    pub(crate) address: IpAddr,
    pub(crate) port: u16,
    pub(crate) max_connections: usize,
    pub(crate) recuse_when_full: bool,
    pub(crate) auto_reconnect: bool,
    pub(crate) no_delay: bool,
    pub(crate) hook_stream: Option<fn(tcp_stream: TcpStream) -> TcpStream>
}

#[allow(dead_code)]
pub struct ListeningInfos{
    pub(crate) split_sink: Arc<Mutex<SplitSink<WebSocketStream<TcpStream>,Message>>>,
    pub(crate) split_stream: Arc<Mutex<SplitStream<WebSocketStream<TcpStream>>>>,
    pub(crate) socket_addr: SocketAddr,
    pub(crate) listening: bool
}

pub struct ServerWebSocket{
    pub(crate) settings: WebSocketSettingsServer,
    pub(crate) connecting: bool,
    pub(crate) connected: bool,
    pub(crate) main_port: bool,
    pub(crate) accepting_connections: bool,
    pub(crate) tcp_listener: Option<Arc<TcpListener>>,

    pub(crate) port_connected_receiver: UnboundedReceiver<TcpListener>,
    pub(crate) port_connected_sender: Arc<UnboundedSender<TcpListener>>,

    pub(crate) client_authenticated_receiver: Arc<Mutex<UnboundedReceiver<Uuid>>>,
    pub(crate) client_authenticated_sender: UnboundedSender<Uuid>,

    pub(crate) connection_down_receiver: UnboundedReceiver<()>,
    pub(crate) connection_down_sender: Arc<UnboundedSender<()>>,

    pub(crate) message_received_receiver: UnboundedReceiver<(Vec<u8>,Uuid)>,
    pub(crate) message_received_sender: Arc<UnboundedSender<(Vec<u8>,Uuid)>>,

    pub(crate) client_disconnected_receiver: UnboundedReceiver<(Uuid,bool)>,
    pub(crate) client_disconnected_sender: Arc<UnboundedSender<(Uuid,bool)>>,

    pub(crate) listening_clients: HashMap<Uuid,ListeningInfos>,
    pub(crate) not_authenticated_listening: HashMap<Uuid,ListeningInfos>,

    pub(crate) client_connected_receiver: UnboundedReceiver<(WebSocketStream<TcpStream>,SocketAddr)>,
    pub(crate) client_connected_sender: Arc<UnboundedSender<(WebSocketStream<TcpStream>,SocketAddr)>>,
}

impl ServerSettingsTrait for WebSocketSettingsServer {
    fn create_server_port(self: Box<Self>) -> Option<Box<dyn ServerPortTrait>> {
        Some(Box::new(ServerWebSocket::new(*self)))
    }
}

impl ServerPortTrait for ServerWebSocket {
    fn connect(&mut self, server_connection: &mut ServerConnection) {
        let runtime = &server_connection.runtime;

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

    fn authenticate_client(&mut self, uuid: Uuid, server_connection: &mut ServerConnection) {
        if self.not_authenticated_listening.contains_key(&uuid){
            let mut listening_infos = self.not_authenticated_listening.remove(&uuid).unwrap();

            listening_infos.listening = true;

            self.listening_clients.insert(uuid, listening_infos);
            self.start_listening_authenticated_client(uuid, server_connection);

            match self.client_authenticated_sender.send(uuid) {
                Ok(_) => {}
                Err(_) => {}
            }
        }
    }

    fn as_main_port(&mut self) -> bool {
        self.main_port = true;

        true
    }

    fn check_messages_received(&mut self) -> (Option<Vec<u8>>, Option<Uuid>) {
        match self.message_received_receiver.try_recv() {
            Ok((bytes,sender)) => {
                (Some(bytes), Some(sender))
            }
            Err(_) => {
                (None, None)
            }
        }
    }

    fn check_port_dropped(&mut self) -> (bool, bool) {
        match self.connection_down_receiver.try_recv() {
            Ok(_) => {
                self.connected = false;
                self.connecting = false;
                self.accepting_connections = false;

                match self.tcp_listener.take() {
                    Some(tcp_listener) => {
                        drop(tcp_listener);
                    },
                    None => {}
                }
                
                (true,self.settings.auto_reconnect)
            }
            Err(_) => {
                (false,false)
            }
        }
    }

    fn check_port_connected(&mut self) {
        if self.connected { return; }

        match self.port_connected_receiver.try_recv() {
            Ok(tcp_listener) => {
                self.connected = true;
                self.tcp_listener = Some(Arc::new(tcp_listener));
            }
            Err(_) => {
                return;
            }
        }
    }

    fn start_accepting_connections(&mut self, server_connection: &mut ServerConnection) {
        let runtime = &server_connection.runtime;

        if let Some(runtime) = runtime {
            if self.accepting_connections || !self.connected {return;}

            self.accepting_connections = true;

            let connection_down_sender = Arc::clone(&self.connection_down_sender);
            let client_connected_sender = Arc::clone(&self.client_connected_sender);
            let settings = &self.settings;
            let no_delay = settings.no_delay;
            let hook_stream = settings.hook_stream;

            let max_connections = self.settings.max_connections;
            let recuse_when_full = self.settings.recuse_when_full;
            let tcp_listener = match &self.tcp_listener {
                Some(tcp_listener) => Arc::clone(&tcp_listener),
                None => { print!("Cant listening for clients because the listener is dropped"); self.accepting_connections = false; return }
            };

            runtime.spawn(async move {
                let semaphore: Option<Semaphore> = if max_connections > 0 { Some(Semaphore::new(max_connections)) } else { None };

                loop {
                    match tcp_listener.accept().await {
                        Ok((mut stream, addr)) => {

                            match stream.set_nodelay(no_delay) {
                                Ok(_) => {}
                                _ => {}
                            }

                            stream = match hook_stream {
                                Some(hook_stream) => {
                                    hook_stream(stream)
                                }
                                None => {
                                    stream
                                }
                            };

                            match &semaphore {
                                Some(semaphore) => {
                                    if recuse_when_full {
                                        match semaphore.try_acquire() {
                                            Ok(permit) => {
                                                drop(permit);

                                                match accept_async(stream).await {
                                                    Ok(web_stream) => {
                                                        client_connected_sender.send((web_stream, addr)).expect("Failed to send new client connected");
                                                    }
                                                    Err(_) => {
                                                        println!("Failed to convert stream to web socket")
                                                    }
                                                };
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

                                                match accept_async(stream).await {
                                                    Ok(web_stream) => {
                                                        client_connected_sender.send((web_stream, addr)).expect("Failed to send new client connected");
                                                    }
                                                    Err(_) => {
                                                        println!("Failed to convert stream to web socket")
                                                    }
                                                };
                                            }
                                            Err(_) => {
                                                drop(stream);
                                            }
                                        }
                                    }
                                }
                                None => {
                                    match accept_async(stream).await {
                                        Ok(web_stream) => {
                                            client_connected_sender.send((web_stream, addr)).expect("Failed to send new client connected");
                                        }
                                        Err(_) => {
                                            println!("Failed to convert stream to web socket")
                                        }
                                    };
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

    fn check_clients_diconnected(&mut self) {
        match self.client_disconnected_receiver.try_recv() {
            Ok((uuid,authenticated)) => {
                let listening_infos = if authenticated {self.listening_clients.get_mut(&uuid)} else {self.not_authenticated_listening.get_mut(&uuid)};

                if let Some(_) = listening_infos{
                    if authenticated {
                        self.listening_clients.remove(&uuid);
                    }else{
                        self.not_authenticated_listening.remove(&uuid);
                    }
                }
            }
            Err(_) => {

            }
        }
    }

    fn check_client_connected(&mut self) {
        match self.client_connected_receiver.try_recv() {
            Ok((web_stream, socket_addr)) => {
                let new_uuid = Uuid::new_v4();
                let (split_sink, split_stream) = web_stream.split();

                let listening_infos = ListeningInfos{
                    split_sink: Arc::new(Mutex::new(split_sink)),
                    split_stream: Arc::new(Mutex::new(split_stream)),
                    socket_addr,
                    listening: false,
                };

                self.not_authenticated_listening.insert(new_uuid, listening_infos);
            }
            Err(_) => {}
        }
    }

    fn check_not_authenticated_clients(&mut self, server_connection: &mut ServerConnection) {
        let mut keys_to_start = Vec::new();

        for (key,infos) in self.not_authenticated_listening.iter_mut(){
            if infos.listening { continue; }

            infos.listening = true;

            keys_to_start.push(*key);
        }

        for key in keys_to_start {
            self.start_listening_not_authenticated_client(
                key,
                server_connection
            );
        }
    }

    fn start_listening_not_authenticated_client(&mut self, target: Uuid, server_connection: &mut ServerConnection) {
        let runtime = &server_connection.runtime;

        if let Some(runtime) = runtime {
            let listening_infos = match self.listening_clients.get_mut(&target) {
                Some(listening_infos) => listening_infos,
                None => {
                    warn!("Client from uuid {} not found ", target);
                    return;
                }
            };
            let split_stream = Arc::clone(&listening_infos.split_stream);
            let client_authenticated_receiver =  Arc::clone(&self.client_authenticated_receiver);
            let client_disconnected_sender = Arc::clone(&self.client_disconnected_sender);
            let message_received_sender = Arc::clone(&self.message_received_sender);

            runtime.spawn(async move {
                let mut guard = split_stream.lock().await;

                loop {
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

                        match guard.next().await {
                            Some(msg) => {
                                match msg {
                                    Ok(Message::Binary(bytes)) => {
                                        message_received_sender.send((Vec::from(bytes), target)).expect("Failed to send message");
                                    }
                                    Err(err) => {
                                        log::error!("Error reading message: {:?}", err);

                                        client_disconnected_sender.send((target, false)).expect("Failed to send client connection down");
                                    }
                                    _ => {

                                    }
                                }
                            }
                            None => {}
                        }
                    }
                }
            });
        }
    }

    fn start_listening_authenticated_client(&mut self, target: Uuid, server_connection: &mut ServerConnection) {
        let runtime = &server_connection.runtime;

        if let Some(runtime) = runtime {
            let listening_infos = match self.listening_clients.get_mut(&target) {
                Some(listening_infos) => listening_infos,
                None => {
                    warn!("Client from uuid {} not found ", target);
                    return;
                }
            };
            let split_stream = Arc::clone(&listening_infos.split_stream);
            let client_disconnected_sender = Arc::clone(&self.client_disconnected_sender);
            let message_received_sender = Arc::clone(&self.message_received_sender);

            runtime.spawn(async move {
                let mut guard = split_stream.lock().await;

                loop {
                    loop {
                        match guard.next().await {
                            Some(msg) => {
                                match msg {
                                    Ok(Message::Binary(bytes)) => {
                                        message_received_sender.send((Vec::from(bytes), target)).expect("Failed to send message");
                                    }
                                    Err(err) => {
                                        log::error!("Error reading message: {:?}", err);

                                        client_disconnected_sender.send((target,true)).expect("Failed to send client connection down");
                                    }
                                    _ => {

                                    }
                                }
                            }
                            None => {}
                        }
                    }
                }
            });
        }
    }

    fn send_message(&mut self, message: Box<&dyn MessageTrait>, target: &Uuid, server_connection: &mut ServerConnection) {
        let runtime = &server_connection.runtime;

        if let Some(runtime) = runtime {
            let listening_infos = match self.listening_clients.get_mut(target) {
                Some(listening_infos) => listening_infos,
                None => {
                    warn!("Client from uuid {} not found ", target);
                    return;
                }
            };

            let split_sink = Arc::clone(&listening_infos.split_sink);
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

impl Default for WebSocketSettingsServer {
    fn default() -> Self {
        WebSocketSettingsServer {
            address: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 8080,
            max_connections: 0,
            recuse_when_full: false,
            auto_reconnect: true,
            no_delay: true,
            hook_stream: None
        }
    }
}

impl Default for ServerWebSocket {
    fn default() -> Self {
        let (port_connected_sender,port_connected_receiver) = unbounded_channel::<TcpListener>();
        let (connection_down_sender,connection_down_receiver) = unbounded_channel();
        let (message_received_sender,message_received_receiver) = unbounded_channel::<(Vec<u8>,Uuid)>();
        let (client_connected_sender,client_connected_receiver) = unbounded_channel::<(WebSocketStream<TcpStream>,SocketAddr)>();
        let (client_authenticated_sender,client_authenticated_receiver) = unbounded_channel::<Uuid>();
        let (client_disconnected_sender,client_disconnected_receiver) = unbounded_channel::<(Uuid,bool)>();

        ServerWebSocket {
            settings: WebSocketSettingsServer::default(),
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

impl WebSocketSettingsServer{
    pub fn new(address: IpAddr, port: u16, max_connections: usize, recuse_when_full: bool, auto_reconnect: bool, no_delay: bool) -> Self{
        WebSocketSettingsServer{
            address,
            port,
            max_connections,
            recuse_when_full,
            auto_reconnect,
            no_delay,
            hook_stream: None
        }
    }
}

impl ServerWebSocket {
    fn new(settings: WebSocketSettingsServer) -> ServerWebSocket {
        let (port_connected_sender,port_connected_receiver) = unbounded_channel::<TcpListener>();
        let (connection_down_sender,connection_down_receiver) = unbounded_channel();
        let (message_received_sender,message_received_receiver) = unbounded_channel::<(Vec<u8>,Uuid)>();
        let (client_connected_sender,client_connected_receiver) = unbounded_channel::<(WebSocketStream<TcpStream>,SocketAddr)>();
        let (client_authenticated_sender,client_authenticated_receiver) = unbounded_channel::<Uuid>();
        let (client_disconnected_sender,client_disconnected_receiver) = unbounded_channel::<(Uuid,bool)>();

        ServerWebSocket {
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
}