use std::{
    io::Write,
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

pub use self::error::{Error, Result};

use clap::Parser;
use serialport::SerialPort;
use wild::ArgsOs;

mod error;

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
        return list_available_ports();
    }
    if let Some(port) = &args.port {
        let sport = open_serial_port(port, &args)?;
        eprintln!("Using serial port: {:#?}", sport);
        // Create thread for serial port reader
        let (serial_reader_tx, serial_reader_rx) = mpsc::channel();
        let serial_reader = thread::spawn(|| {
            handle_serial_port(sport, serial_reader_tx);
        });
        let tcp_write_senders = Arc::new(Mutex::new(Vec::new()));
        // Create thread for TCP listener
        let tcp_write_senders_for_listener = Arc::clone(&tcp_write_senders);
        let listener_thread = thread::spawn(move || {
            handle_tcp_listener(&args.listen, tcp_write_senders_for_listener);
        });
        // Read data for serial port and dispatch to each TCP stream writer
        for buf in serial_reader_rx {
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

/// Print a list of available serial ports to console
fn list_available_ports() -> Result<()> {
    let ports = serialport::available_ports()?;
    if ports.is_empty() {
        eprintln!("No serial ports detected!");
        return Ok(());
    }
    //println!("{:#?}", ports);
    for port in ports {
        match &port.port_type {
            serialport::SerialPortType::UsbPort(p) => {
                println!(
                    "{} - USB: ID {:04x}:{:04x} {} {} {}",
                    port.port_name,
                    p.vid,
                    p.pid,
                    p.manufacturer.clone().unwrap_or_default(),
                    p.product.clone().unwrap_or_default(),
                    p.serial_number
                        .clone()
                        .map(|s| ["serial", s.as_ref()].join(" "))
                        .unwrap_or_default(),
                )
            }
            serialport::SerialPortType::PciPort => {
                println!("{} - PCI: {:?}", port.port_name, port.port_type)
            }
            serialport::SerialPortType::BluetoothPort => {
                println!("{} - BT: {:?}", port.port_name, port.port_type)
            }
            serialport::SerialPortType::Unknown => {
                println!("{} - {:?}", port.port_name, port.port_type)
            }
        }
    }
    Ok(())
}

/// Configure and open serial port using CLI arguments
///
/// ESP32 uses 115200 8N1
/// MBUS slave uses 2400 8E1
fn open_serial_port(port: &str, args: &Args) -> Result<Box<dyn SerialPort>> {
    let mut builder = serialport::new(port, args.baud_rate);
    builder = builder.data_bits(match args.data_bits {
        8 => serialport::DataBits::Eight,
        7 => serialport::DataBits::Seven,
        6 => serialport::DataBits::Six,
        5 => serialport::DataBits::Five,
        _ => {
            eprintln!("Unsupported data bits, using 8");
            serialport::DataBits::Eight
        }
    });
    builder = builder.parity(match args.parity {
        'N' => serialport::Parity::None,
        'O' => serialport::Parity::Odd,
        'E' => serialport::Parity::Even,
        _ => {
            eprintln!("Unsupported parity, using N");
            serialport::Parity::None
        }
    });
    builder = builder.stop_bits(match args.stop_bits {
        1 => serialport::StopBits::One,
        2 => serialport::StopBits::Two,
        _ => {
            eprintln!("Unsupported stop bits, using 1");
            serialport::StopBits::One
        }
    });
    builder = builder.flow_control(match args.flow_control {
        'N' => serialport::FlowControl::None,
        'H' => serialport::FlowControl::Hardware,
        'S' => serialport::FlowControl::Software,
        _ => {
            eprintln!("Unsupported flow control, using N");
            serialport::FlowControl::None
        }
    });
    Ok(builder.open()?)
}

fn handle_serial_port(mut port: Box<dyn SerialPort>, tx: Sender<Vec<u8>>) {
    loop {
        let mut buf = [0; 1024];
        match port.read(&mut buf) {
            Ok(n) => {
                let v = buf[..n].to_vec();
                match tx.send(v) {
                    Ok(_) => continue,
                    Err(e) => {
                        eprintln!("Error sending data from serial port reader: {}", e);
                        break;
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => {
                eprintln!("Reading from serial port failed: {}", e);
                break;
            }
        }
    }
}

fn handle_tcp_listener(bind_addr: &str, tcp_write_senders: Arc<Mutex<Vec<Sender<Vec<u8>>>>>) {
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

fn handle_tcp_stream(mut stream: TcpStream, tcp_writer_rx: Receiver<Vec<u8>>) {
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
