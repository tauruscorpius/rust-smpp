#![cfg(test)]

use std::io;

pub struct FailingRead {}

impl FailingRead {
    fn error() -> io::Error {
        io::Error::from_raw_os_error(22)
    }
}

impl io::Read for FailingRead {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(FailingRead::error())
    }
}
