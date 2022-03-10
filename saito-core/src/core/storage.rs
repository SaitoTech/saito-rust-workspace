use crate::common::handle_io::HandleIo;

pub struct Storage {
    io_handler: dyn HandleIo,
}
