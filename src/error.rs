use derive_more::From;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, From)]
pub enum Error {
    #[from]
    Io(std::io::Error),
    #[from]
    SerialPort(serialport::Error),
    #[from]
    MpscRecv(std::sync::mpsc::RecvError),
    #[from]
    ThreadJoin(Box<dyn std::any::Any + Send>),
}

impl std::error::Error for Error {}

impl core::fmt::Display for Error {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
        write!(fmt, "{self:?}")
    }
}
