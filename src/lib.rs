use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
};

pub use self::error::{Error, Result};

use clap::Parser;
use wild::ArgsOs;

mod error;
mod serial;
mod tcp;

type Avb = Arc<Vec<u8>>;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Serial port to connect to
    #[arg()]
    port: Option<String>,
    /// Bind to host:port
    #[arg(default_value = "127.0.0.1:4567")]
    listen: String,
    /// List available serial ports
    #[arg(short('L'), long)]
    list_available_ports: bool,
    /// Baud rate
    #[arg(short('b'), long, default_value_t = 115_200)]
    baud_rate: u32,
    /// Data bits
    ///
    /// Valid values: 5, 6, 7, 8
    #[arg(short('d'), long, default_value_t = 8)]
    data_bits: u8,
    /// Parity
    ///
    /// Valid values: [N]one, [O]dd, [E]ven
    #[arg(short('p'), long, default_value_t = 'N')]
    parity: char,
    /// Stop bits
    ///
    /// Valid values: 1, 2
    #[arg(short('s'), long, default_value_t = 1)]
    stop_bits: u8,
    /// Flow control
    ///
    /// Valid values: [N]one, [H]ardware, [S]oftware
    #[arg(short('f'), long, default_value_t = 'N')]
    flow_control: char,
}

pub fn run(args: ArgsOs) -> Result<()> {
    let args = Args::parse_from(args);
    //println!("{args:?}");
    if args.port.is_none() || args.list_available_ports {
        return serial::list_available_ports();
    }
    if let Some(port) = &args.port {
        let sport = serial::open_serial_port(port, &args)?;
        eprintln!("Using serial port: {:#?}", sport);
        // Create thread for serial port reader
        let (serial_reader_tx, serial_reader_rx) = mpsc::channel();
        let serial_reader = thread::spawn(|| {
            serial::handle_serial_port(sport, serial_reader_tx);
        });
        let tcp_write_senders = Arc::new(Mutex::new(Vec::new()));
        // Create thread for TCP listener
        let tcp_write_senders_for_listener = Arc::clone(&tcp_write_senders);
        let listener_thread = thread::spawn(move || {
            tcp::handle_tcp_listener(&args.listen, tcp_write_senders_for_listener);
        });
        // Read data for serial port and dispatch to each TCP stream writer
        for buf in serial_reader_rx {
            let buf = Arc::new(buf);
            //print!("{}", String::from_utf8_lossy(buf.as_slice()));
            let Ok(mut tcp_write_senders) = tcp_write_senders.lock() else {
                continue;
            };
            // Send data and remove sender if error occurs
            tcp_write_senders.retain_mut(|tx| tx.send(buf.clone()).is_ok());
        }
        serial_reader.join()?;
        listener_thread.join()?;
    }
    Ok(())
}
