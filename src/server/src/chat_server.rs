use std::collections::HashMap;
use std::io::{ErrorKind, Result};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, mpsc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::thread::JoinHandle;

use crate::chat_event_handler::{ChatEventHandler};
use log::{info};


#[derive(Debug)]
pub enum ChannelMessage {
    RegisterConnection(SocketAddr, TcpStream),
    Broadcast(SocketAddr, String),
    TerminateConnection(SocketAddr),
    Exit,
}
type LockedReceiver = Arc<Mutex<Receiver<ChannelMessage>>>;
pub type LockedSender = Arc<Mutex<Sender<ChannelMessage>>>;
type LockedThreadList = Arc<Mutex<Vec<Option<JoinHandle<()>>>>>;
type LockedClientsMap = Arc<Mutex<HashMap<SocketAddr, TcpStream>>>;

pub struct ChatServer {
    listen_socket: TcpListener,
    exit_flag: Arc<AtomicBool>,
    pub(crate) clients: LockedClientsMap,
    pub(crate) receiver: LockedReceiver,
    pub(crate) sender: LockedSender,
    pub(crate) thread_handles: LockedThreadList,
}


impl ChatServer {
    pub fn new(str_addr: &str, event_handler: Box<dyn ChatEventHandler + Send>) -> Result<Arc<Self>> {
        let listen_socket = TcpListener::bind(str_addr)?;

        listen_socket.set_nonblocking(true).expect("Failed to set unblocking");

        let (sender, receiver) = mpsc::channel::<ChannelMessage>();
        let sender = Arc::new(Mutex::new(sender));
        let receiver = Arc::new(Mutex::new(receiver));


        let new_server = Arc::new(Self {
            listen_socket,
            exit_flag: Arc::new(AtomicBool::from(false)),
            sender,
            receiver,
            clients: Arc::new(Mutex::new(HashMap::new())),
            thread_handles: Arc::new(Mutex::new(Vec::new()))
        });

        info!("Listening on {:?}", new_server.listen_socket.local_addr().unwrap());

        let server_copy = new_server.clone();

        thread::spawn(move || {
            info!("Started event loop handler");
            event_handler.handle_messages(server_copy);
        });

        Ok(new_server)
    }

    pub fn kill_switch(&self) {
        info!("Exit flag turned on");
        self.exit_flag.store(true, Ordering::SeqCst);
    }

    pub fn main_loop(&self) {
        let sender = self.sender.clone();

        for stream in self.listen_socket.incoming() {
            match stream {
                Ok(stream) => {
                    info!("New connection: {:?}", stream.peer_addr().unwrap());
                    sender.lock()
                        .unwrap()
                        .send(ChannelMessage::RegisterConnection(stream.peer_addr().unwrap(), stream)).expect("Failed to send on channel");
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    if self.exit_flag.load(Ordering::SeqCst) {
                        info!("Sending exit command to threads");
                        sender.lock()
                            .unwrap()
                            .send(ChannelMessage::Exit).expect("Failed to send on channel");

                        info!("Joining all threads");
                        self.thread_handles.lock().unwrap().iter_mut().for_each(|handle| {
                            handle.take().unwrap().join().ok();
                        });
                        break;
                    }
                }
                Err(e) => panic!("Unexpected IO Error: {:?}", e)
            }
        }
    }
}