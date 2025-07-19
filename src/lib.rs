use std::{
    sync::mpsc::{self, Sender},
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
        let (serial_reader_tx, serial_reader_rx) = mpsc::channel();
        let serial_reader = thread::spawn(|| {
            handle_serial_port(sport, serial_reader_tx);
        });
        for buf in serial_reader_rx {
            print!("{}", String::from_utf8_lossy(buf.as_slice()));
        }
        serial_reader.join()?;
    }
    Ok(())
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
