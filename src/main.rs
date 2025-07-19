use ser2tcp::Result;

fn main() -> Result<()> {
    let args = wild::args_os();
    ser2tcp::run(args)
}