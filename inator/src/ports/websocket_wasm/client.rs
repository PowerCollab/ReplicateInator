#![allow(dead_code)]
#![allow(unused_imports)]

use std::cell::RefCell;
use std::rc::Rc;
use bevy::prelude::warn;
use bevy::reflect::erased_serde::__private::serde::de::DeserializeOwned;
use bevy::reflect::erased_serde::__private::serde::Serialize;
use ciborium::into_writer;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use gloo_net::websocket::futures::WebSocket;
use gloo_net::websocket::Message;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use wasm_bindgen_futures::spawn_local;
use crate::plugins::connection::ClientConnection;
use crate::plugins::messaging::MessageTrait;
use crate::ports::{ClientPortTrait, ClientSettingsTrait};

pub struct WasmSettingsClient{
    pub(crate) url: String,
    pub(crate) try_reconnect: bool,
    pub(crate) websocket_hook: Option<fn(web_socket: WebSocket) -> WebSocket>
}

pub struct WasmClient{
    pub(crate) settings: WasmSettingsClient,
    pub(crate) connecting: bool,
    pub(crate) connected: bool,
    pub(crate) listening: bool,
    pub(crate) main_port: bool,

    pub(crate) sending_messages: Rc<RefCell<bool>>,

    pub(crate) connected_to_server_receiver: UnboundedReceiver<WebSocket>,
    pub(crate) connected_to_server_sender: Rc<UnboundedSender<WebSocket>>,

    pub(crate) connection_down_receiver: UnboundedReceiver<()>,
    pub(crate) connection_down_sender: Rc<UnboundedSender<()>>,

    pub(crate) message_received_receiver: UnboundedReceiver<Vec<u8>>,
    pub(crate) message_received_sender: Rc<UnboundedSender<Vec<u8>>>,

    pub(crate) message_queue_receiver: Rc<RefCell<UnboundedReceiver<Vec<u8>>>>,
    pub(crate) message_queue_sender: UnboundedSender<Vec<u8>>,

    pub(crate) split_sink: Option<Rc<RefCell<SplitSink<WebSocket, Message>>>>,
    pub(crate) split_stream: Option<Rc<RefCell<SplitStream<WebSocket>>>>,
}

impl Default for WasmSettingsClient {
    fn default() -> Self {
        WasmSettingsClient {
            url: "ws://127.0.0.1:1234".to_string(),
            try_reconnect: true,
            websocket_hook: None
        }
    }
}

impl Default for WasmClient {
    fn default() -> Self {
        let (message_received_sender,message_received_receiver) = unbounded_channel::<Vec<u8>>();
        let (message_queue_sender,message_queue_receiver) = unbounded_channel::<Vec<u8>>();
        let (connected_to_server_sender,connected_to_server_receiver) = unbounded_channel::<WebSocket>();
        let (connection_down_sender,connection_down_receiver) = unbounded_channel();

        WasmClient{
            settings: WasmSettingsClient::default(),
            connecting: false,
            connected: false,
            listening: false,
            main_port: false,

            sending_messages: Rc::new(RefCell::new(false)),

            connected_to_server_receiver,
            connected_to_server_sender: Rc::new(connected_to_server_sender),

            connection_down_receiver,
            connection_down_sender: Rc::new(connection_down_sender),

            message_received_receiver,
            message_received_sender: Rc::new(message_received_sender),

            message_queue_receiver: Rc::new(RefCell::new(message_queue_receiver)),
            message_queue_sender,

            split_sink: None,
            split_stream: None
        }
    }
}

impl WasmSettingsClient {
    pub fn new(url: String, try_reconnect: bool) -> Self {
        WasmSettingsClient{
            url,
            try_reconnect,
            websocket_hook: None
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl ClientSettingsTrait for WasmSettingsClient {
    fn create_client_port(self: Box<Self>) -> Option<Box<dyn ClientPortTrait>> {
        Some(Box::new(WasmClient::new(*self)))
    }
}

#[cfg(target_arch = "wasm32")]
impl ClientPortTrait for WasmClient{
    fn connect(&mut self, _: &mut ClientConnection) {
        if self.connecting {return;}

        self.connecting = true;

        let settings = &self.settings;
        let websocket_hook = settings.websocket_hook;
        let url = settings.url.clone();

        let connected_to_server_sender = Rc::clone(&self.connected_to_server_sender);
        let connection_down_sender = Rc::clone(&self.connection_down_sender);

        let try_reconnect = settings.try_reconnect;

        spawn_local(async move {
            loop {
                let web_socket = WebSocket::open(&url);

                match web_socket {
                    Ok(mut web_socket) => {

                        web_socket = match websocket_hook {
                            Some(websocket_hook) => {
                                websocket_hook(web_socket)
                            }
                            None => {
                                web_socket
                            }
                        };

                        connected_to_server_sender.send(web_socket).expect("Failed to send web socket connected");
                        return;
                    }
                    _ => {
                        if try_reconnect {
                            warn!("Failed to open websocket, retrying");
                        }else{
                            warn!("Failed to open websocket, closing all");
                            connection_down_sender.send(()).expect("Failed to send connection down");
                            return;
                        }
                    }
                }
            }
        })
    }

    fn as_main_port(&mut self) -> bool {
        self.main_port = true;
        true
    }

    fn check_messages_received(&mut self) -> Option<Vec<u8>> {
        match self.message_received_receiver.try_recv() {
            Ok(bytes) => {
                Some(bytes)
            }
            Err(_) => {
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

    fn check_connected_to_server(&mut self) {
        if self.connected { return; }

        match self.connected_to_server_receiver.try_recv() {
            Ok(web_socket) => {
                self.connected = true;

                let (sink,stream) = web_socket.split();

                self.split_sink = Some(Rc::new(RefCell::new(sink)));
                self.split_stream = Some(Rc::new(RefCell::new(stream)));
            }
            Err(_) => {}
        }
    }

    fn start_listening_to_server(&mut self, _: &mut ClientConnection) {
        if self.listening { return; }

        self.listening = true;

        if let Some(stream) = &self.split_stream {
            let stream = Rc::clone(&stream);

            let connection_down_sender = Rc::clone(&self.connection_down_sender);
            let message_received_sender = Rc::clone(&self.message_received_sender);

            spawn_local(async move {
                let mut stream_mut = stream.borrow_mut();

                loop {
                    match stream_mut.next().await {
                        None => {
                            connection_down_sender.send(()).expect("Failed to send connection down");
                            return;
                        }
                        Some(msg) => {
                            match msg {
                                Ok(Message::Bytes(bytes)) => {
                                    message_received_sender.send(bytes).expect("Failed to send message");
                                }
                                Ok(Message::Text(text)) => {
                                    log::warn!("Unexpected text : {}", text);
                                }
                                Err(err) => {
                                    log::error!("Error reading message: {:?}", err);

                                    connection_down_sender.send(()).expect("Failed to send connection down");
                                    return;
                                }
                            }
                        }
                    }
                }
            })
        }
    }

    fn send_message(&mut self, message: Box<&dyn MessageTrait>, _: &mut ClientConnection) {
        let mut buf = Vec::new();

        match into_writer(&message,&mut buf) {
            Ok(_) => {}
            Err(_) => {
                warn!("Error to serialize message");
                return;
            }
        };

        self.message_queue_sender.send(buf).expect("Failed to send message to queue");

        if let Some(sink) = &self.split_sink {
            if *self.sending_messages.borrow() { return; }

            *self.sending_messages.borrow_mut() = true;

            let message_queue_receiver = Rc::clone(&self.message_queue_receiver);
            let sink = Rc::clone(&sink);
            let sending_messages = Rc::clone(&self.sending_messages);

            spawn_local(async move {
                loop {
                    let opt_bytes = {
                        let mut receiver = message_queue_receiver.borrow_mut();
                        match receiver.try_recv() {
                            Ok(bytes) => Some(bytes),
                            Err(_) => None,
                        }
                    };

                    let bytes = match opt_bytes {
                        Some(b) => b,
                        None => break,
                    };

                    {
                        let mut sink_mut = sink.borrow_mut();
                        if let Err(e) = sink_mut.send(Message::Bytes(bytes)).await {
                            warn!("Error sending message: {:?}", e);
                            break;
                        }
                    }
                }

                *sending_messages.borrow_mut() = false;
            });
        }
    }
}

impl WasmClient {
    fn new(settings: WasmSettingsClient) -> Self {
        let (message_received_sender,message_received_receiver) = unbounded_channel::<Vec<u8>>();
        let (message_queue_sender,message_queue_receiver) = unbounded_channel::<Vec<u8>>();
        let (connected_to_server_sender,connected_to_server_receiver) = unbounded_channel::<WebSocket>();
        let (connection_down_sender,connection_down_receiver) = unbounded_channel();

        WasmClient{
            settings,
            connecting: false,
            connected: false,
            listening: false,
            main_port: false,

            sending_messages: Rc::new(RefCell::new(false)),

            connected_to_server_receiver,
            connected_to_server_sender: Rc::new(connected_to_server_sender),

            connection_down_receiver,
            connection_down_sender: Rc::new(connection_down_sender),

            message_received_receiver,
            message_received_sender: Rc::new(message_received_sender),

            message_queue_receiver: Rc::new(RefCell::new(message_queue_receiver)),
            message_queue_sender,

            split_sink: None,
            split_stream: None
        }
    }
}
