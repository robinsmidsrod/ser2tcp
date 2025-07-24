use std::io::Write; // for write_all()
use std::net::TcpListener;
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
use std::sync::mpsc::Sender;
use std::thread;

use super::Avb;

/// Create TCP listener on specified bind address+port
///
/// Create a channel for each connected TCP client and add it to the list of serial data receivers
pub(crate) fn handle_tcp_listener(
    bind_addr: &str,
    tcp_write_senders: Arc<Mutex<Vec<Sender<Avb>>>>,
) {
    let tcp_listener = TcpListener::bind(bind_addr);
    let Ok(tcp_listener) = tcp_listener else {
        return;
    };
    let Ok(local_addr) = tcp_listener.local_addr() else {
        return;
    };
    eprintln!("Listening on: {local_addr}");
    let mut tcp_stream_threads = Vec::new();
    for stream in tcp_listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        let (tcp_write_tx, tcp_write_rx) = mpsc::channel();
        {
            let Ok(mut tcp_write_senders) = tcp_write_senders.lock() else {
                continue;
            };
            tcp_write_senders.push(tcp_write_tx);
        }
        let thread = thread::spawn(move || {
            handle_tcp_stream(stream, tcp_write_rx);
        });
        tcp_stream_threads.push(thread);
    }
    for thread in tcp_stream_threads {
        match thread.join() {
            Ok(_) => continue,
            Err(e) => {
                eprintln!("Unable to join TCP stream thread: {e:?}");
                continue;
            }
        }
    }
}

/// Write received data from the serial reader channel to the TCP stream
pub(crate) fn handle_tcp_stream(mut stream: TcpStream, tcp_writer_rx: Receiver<Avb>) {
    let Ok(peer_addr) = stream.peer_addr() else {
        return;
    };
    eprintln!("New connection from: {peer_addr}");
    for buf in tcp_writer_rx {
        match stream.write_all(buf.as_slice()) {
            Ok(_) => continue,
            Err(e) => {
                eprintln!("Closed connection from: {peer_addr}: {e}");
                break;
            }
        }
    }
}
