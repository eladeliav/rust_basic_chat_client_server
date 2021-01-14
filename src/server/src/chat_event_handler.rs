use crate::chat_server::{ChannelMessage, LockedSender, ChatServer};
use std::thread;
use std::net::{Shutdown, TcpStream, SocketAddr};
use std::io::{ErrorKind, Read, Write};
use std::sync::{Arc};
use std::borrow::BorrowMut;
use log::{info, error};

pub const DEFAULT_BUFFER_SIZE: usize = 1024;
const SERVER_SHUTDOWN_MESSAGE: &str = "Server shutting down...";

pub trait ChatEventHandler {
    fn handle_messages(&self, _: Arc<ChatServer>);
}


pub struct BasicChatEventHandler;

fn handle_single_client(mut stream: TcpStream, sender: &LockedSender) {
    loop {
        let mut buffer = vec![0; DEFAULT_BUFFER_SIZE];

        match stream.read(&mut buffer) {
            Ok(_) => {
                let msg: Vec<u8> = buffer.into_iter().take_while(|&x| x != 0).collect();
                let msg = String::from_utf8(msg).expect("Received Invalid Message");
                if msg.len() > 0 {
                    sender.lock().unwrap().send(ChannelMessage::Broadcast(stream.peer_addr().unwrap(), msg)).ok();
                } else {
                    sender.lock().unwrap().send(ChannelMessage::TerminateConnection(stream.peer_addr().unwrap())).ok();
                    return;
                }
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => (),
            Err(_) => {
                sender.lock().unwrap().send(ChannelMessage::TerminateConnection(stream.peer_addr().unwrap())).ok();
                return;
            }
        }
    }
}

fn register_new_client(server: &Arc<ChatServer>, sock_addr: SocketAddr, stream: TcpStream) {
    let stream_copy = stream.try_clone().unwrap();
    let sender_copy = server.sender.clone();

    let client_joined_msg = String::from("HAS JOINED THE ROOM\n");
    server.sender.lock().unwrap().send(ChannelMessage::Broadcast(sock_addr, client_joined_msg)).unwrap();

    let thread_handle = thread::spawn(move ||
        handle_single_client(stream_copy, &sender_copy)
    );

    server.thread_handles.lock().unwrap().push(Option::from(thread_handle));
    server.clients.lock().unwrap().insert(sock_addr, stream);
}

fn broadcast(server: &Arc<ChatServer>, sock_addr: SocketAddr, msg: String){
    server.clients.lock().unwrap().borrow_mut().iter().for_each(|(addr, mut stream)| {
        if sock_addr != *addr {
            stream.write_all(msg.as_bytes()).ok();
        }
    });
}

fn terminate_connection(server: &Arc<ChatServer>, sock_addr: SocketAddr){
    let mut guard = server.clients.lock().unwrap();
    if let Some(found_stream) = guard.get(&sock_addr)
    {
        let sock_addr = found_stream.peer_addr().unwrap();
        let client_left_msg = String::from("HAS LEFT THE ROOM\n");
        server.sender.lock().unwrap().send(ChannelMessage::Broadcast(sock_addr, client_left_msg)).unwrap();
        //broadcast(server, sock_addr, client_left_msg);
        found_stream.shutdown(Shutdown::Both).ok();
        guard.remove(&sock_addr);
    }
    drop(guard);
}

impl ChatEventHandler for BasicChatEventHandler {
    fn handle_messages(&self, server: Arc<ChatServer>) {
        for message in server.receiver.lock().unwrap().iter() {
            info!("Event: {:?}", message);
            match message {
                ChannelMessage::RegisterConnection(sock_addr, stream) => register_new_client(&server, sock_addr, stream),
                ChannelMessage::Broadcast(sock_addr, msg) => {
                    let msg = format!("{}: {}", sock_addr, msg);
                    broadcast(&server, sock_addr, msg)
                },
                ChannelMessage::TerminateConnection(sock_addr) => terminate_connection(&server, sock_addr),
                ChannelMessage::Exit => {
                    info!("Shutting down all client connections...");
                    server.clients.lock().unwrap().iter().for_each(|(_, mut stream)| {
                        stream.write_all(SERVER_SHUTDOWN_MESSAGE.as_bytes()).ok();
                        stream.write("".as_bytes()).unwrap();
                        stream.shutdown(Shutdown::Both).unwrap_or(
                            error!("Failed to close socket to: {:?}", stream.peer_addr().unwrap())
                        );
                    });
                    return;
                }
            }
        }
    }
}
