extern crate ctrlc;
extern crate log;
extern crate simple_logger;

mod chat_event_handler;
mod chat_server;


use simple_logger::SimpleLogger;
use chat_server::ChatServer;
use chat_event_handler::BasicChatEventHandler;
use std::process::exit;


const ACCEPT_ALL_ADDR: &str = "0.0.0.0:1234";

fn main() {
    SimpleLogger::new().init().unwrap();

    let event_handler = Box::new(BasicChatEventHandler);
    let shared_server = ChatServer::new(ACCEPT_ALL_ADDR, event_handler).unwrap_or_else(|e| {
        log::error!("Failed to create valid socket: {:?}", e);
        exit(1);
    });

    let server_clone = shared_server.clone();

    ctrlc::set_handler(move || {
       server_clone.kill_switch();
    }).expect("Failed to set signal handler");

    shared_server.main_loop();
}
