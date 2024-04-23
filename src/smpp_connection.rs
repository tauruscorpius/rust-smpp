use ascii::AsciiString;
use bytes::{Buf, BytesMut};
use log::*;
use smpp_pdu::pdu::{CheckOutcome, Pdu, PduParseError, PduParseErrorBody};
use std::io;
use std::io::Cursor;
use std::net::SocketAddr;
use tokio::io::{split, AsyncReadExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

#[derive(Clone, Eq, Hash, PartialEq)]
pub struct EsmeId {
    pub system_id: AsciiString,
    pub system_type: AsciiString,
}

pub struct SmppConnection {
    pub socket_addr: SocketAddr,
    read: Mutex<Option<SmppRead>>,
    write: Mutex<Option<SmppWrite>>,
    bound_esme_id: std::sync::Mutex<Option<EsmeId>>,
}

impl SmppConnection {
    pub fn new(
        tcp_stream: TcpStream,
        socket_addr: SocketAddr,
    ) -> SmppConnection {
        let (read_stream, write_stream) = split(tcp_stream);
        let buffer = BytesMut::with_capacity(4096);
        let read = SmppRead {
            stream: read_stream,
            buffer,
        };
        let write = SmppWrite {
            stream: write_stream,
        };
        SmppConnection {
            read: Mutex::new(Some(read)),
            write: Mutex::new(Some(write)),
            socket_addr,
            bound_esme_id: std::sync::Mutex::new(None),
        }
    }

    pub fn bound_esme_id(&self) -> Option<EsmeId> {
        self.bound_esme_id.lock().unwrap().clone()
    }

    pub async fn bind(&self, system_id: AsciiString, system_type: AsciiString) {
        self.bound_esme_id.lock().unwrap().replace(EsmeId {
            system_id,
            system_type,
        });
    }

    pub async fn read_pdu(&self) -> Result<Option<Pdu>, PduParseError> {
        loop {
            let mut read = self.read.lock().await;
            if let Some(read) = &mut *read {
                if let Some(pdu) = read.parse_pdu()? {
                    return Ok(Some(pdu));
                }

                if 0 == read.read_own_buf().await? {
                    if read.buffer.is_empty() {
                        return Ok(None);
                    } else {
                        return Err(PduParseError::new(
                            PduParseErrorBody::NotEnoughBytes,
                        ));
                    }
                }
            } else {
                error!("Attempting to read from a closed connection!");
                return Err(PduParseError::new(
                    PduParseErrorBody::NotEnoughBytes.into(),
                ));
            }
        }
    }

    pub async fn write_pdu(&self, pdu: &Pdu) -> io::Result<()> {
        info!("=> {} {:?}", self.socket_addr, pdu);
        if let Some(write) = &mut *self.write.lock().await {
            pdu.write(&mut write.stream).await
        } else {
            error!("Attempting to write to a closed connection!");
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }

    pub async fn disconnect(&self) {
        self.read.lock().await.take();
        self.write.lock().await.take();
    }
}

struct SmppRead {
    stream: ReadHalf<TcpStream>,
    buffer: BytesMut,
}

impl SmppRead {
    async fn read_own_buf(&mut self) -> Result<usize, io::Error> {
        self.stream.read_buf(&mut self.buffer).await
    }

    fn parse_pdu(&mut self) -> Result<Option<Pdu>, PduParseError> {
        let mut buf = Cursor::new(&self.buffer[..]);
        match Pdu::check(&mut buf) {
            Ok(CheckOutcome::Ready) => {
                // Pdu::check moved us to the end, so position is length
                let len = buf.position() as usize;

                // Rewind and parse
                buf.set_position(0);
                let pdu = Pdu::parse(&mut buf)?;

                // Parsing succeeded, so consume bytes from buffer and return
                self.buffer.advance(len);
                Ok(Some(pdu))
            }
            // Try again when we have more
            Ok(CheckOutcome::Incomplete) => Ok(None),
            // Failed (e.g. too long)
            Err(e) => Err(e.into()),
            // Issue#1: it would be good to respond with a specific error here,
            // instead of generic_nack.  That should be possible in some cases
            // if we can read the PDU header before we reject it.  It's not
            // too bad to do this though, because the PDU is actually
            // malformed, so not knowing what type it is is forgivable.
        }
    }
}

struct SmppWrite {
    stream: WriteHalf<TcpStream>,
}

impl SmppWrite {}
