use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use bevy::platform::collections::HashMap;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::{Mutex, Semaphore};
use tokio_tungstenite::{accept_async, WebSocketStream};
use uuid::Uuid;
use crate::ports::tcp::server::ListeningInfos;

pub struct WebSocketSettingsServer{
    pub(crate) address: IpAddr,
    pub(crate) port: u16,
    pub(crate) max_connections: usize,
    pub(crate) recuse_when_full: bool,
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

impl ServerWebSocket {
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
                None => { print!("Cant listening for clients becasue the listenner is dropped"); self.accepting_connections = false; return }
            };

            runtime.spawn(async move {
                let semaphore: Option<Semaphore> = if max_connections > 0 { Some(Semaphore::new(max_connections)) } else { None };

                loop {
                    match tcp_listener.accept().await {
                        Ok((stream, addr)) => {
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

    pub fn with_main_port(mut self) -> Self {
        self.main_port = true;
        self
    }
}