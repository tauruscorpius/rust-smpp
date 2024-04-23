use async_trait::async_trait;
use once_cell::sync::Lazy;
use smpp::async_result::AsyncResult;
use smpp::message_unique_key::MessageUniqueKey;
use smpp::smsc::{
    BindData, BindError, Smsc, SmscConfig, SmscLogic, SubmitSmError,
};
use smpp_pdu::pdu::{Pdu, SubmitSmPdu, SubmitSmRespPdu};
use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

const TEST_BIND_URL: &str = "127.0.0.1";

static PORT: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(8080));

pub struct DefaultLogic {}

#[async_trait]
impl SmscLogic for DefaultLogic {
    async fn bind(&mut self, _bind_data: &BindData) -> Result<(), BindError> {
        Ok(())
    }

    async fn submit_sm(
        &mut self,
        _smsc: Arc<Mutex<Smsc>>,
        _pdu: &SubmitSmPdu,
        _sequence_number: u32,
    ) -> Result<(SubmitSmRespPdu, MessageUniqueKey), SubmitSmError> {
        Err(SubmitSmError::InternalError)
    }
}

/// Setup for running tests that send and receive PDUs
pub struct TestSetup {
    pub server: TestServer,
    pub client: TestClient,
}

#[allow(dead_code)]
impl TestSetup {
    pub async fn new() -> TestSetup {
        let server = TestServer::start().await.unwrap();
        let client = TestClient::connect_to(&server).await.unwrap();

        TestSetup { server, client }
    }

    pub async fn new_with_logic<L: SmscLogic + Send + Sync + 'static>(
        smsc_logic: L,
    ) -> Self {
        let server = TestServer::start_with_logic(smsc_logic).await.unwrap();
        let client = TestClient::connect_to(&server).await.unwrap();
        Self { server, client }
    }

    pub async fn new_client(&mut self) {
        self.client = TestClient::connect_to(&self.server).await.unwrap();
    }
}

fn next_port() -> usize {
    return PORT.fetch_add(1, Ordering::Relaxed);
}

/// A test server listening on the test port
pub struct TestServer {
    pub smsc: Arc<Mutex<Smsc>>,
    pub bind_address: String,
}

#[allow(dead_code)]
impl TestServer {
    pub async fn start() -> AsyncResult<TestServer> {
        let logic = DefaultLogic {};
        TestServer::start_with_logic(logic).await
    }

    pub async fn start_with_logic<L: SmscLogic + Send + Sync + 'static>(
        smsc_logic: L,
    ) -> AsyncResult<Self> {
        TestServer::start_with_logic_and_config(smsc_logic, 2).await
    }

    pub async fn start_with_logic_and_config<
        L: SmscLogic + Send + Sync + 'static,
    >(
        smsc_logic: L,
        max_open_sockets: usize,
    ) -> AsyncResult<Self> {
        let _ = env_logger::builder()
            .filter_level(log::LevelFilter::Trace)
            .is_test(true)
            .try_init();

        let bind_address = format!("{}:{}", TEST_BIND_URL, next_port());

        let smsc_config = SmscConfig {
            bind_address: String::from(&bind_address),
            max_open_sockets,
            system_id: String::from("TestServer"),
        };

        let smsc = Smsc::start(smsc_config, smsc_logic).await.unwrap();

        let server = TestServer { smsc, bind_address };

        // Force the runtime to actually do something: seems to mean
        // the server is running when we connect to it.  Hopefully
        // there is a better way?
        sleep(Duration::from_millis(1)).await;

        Ok(server)
    }

    pub async fn receive_pdu(
        &self,
        namespace_id: &str,
        pdu: Pdu,
    ) -> AsyncResult<()> {
        self.smsc.lock().await.receive_pdu(namespace_id, pdu).await
    }
}

/// A client that is able to connect to the server
pub struct TestClient {
    pub stream: TcpStream,
}

#[allow(dead_code)]
impl TestClient {
    pub async fn connect_to(server: &TestServer) -> AsyncResult<TestClient> {
        // Connect to the server, retrying with 10ms delay if we fail
        let mut i: u8 = 0;
        loop {
            match TcpStream::connect(&server.bind_address).await {
                Ok(stream) => return Ok(TestClient { stream }),
                Err(e) => {
                    i += 1;
                    sleep(Duration::from_millis(10)).await;
                    if i > 9 {
                        return Err(e.into());
                    }
                }
            }
        }
    }

    pub async fn bind_receiver(&mut self) {
        self.send_and_expect_response(
            b"\x00\x00\x00\x29\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x07\
        esmeid\0password\0type\0\x34\x00\x00\0",
            b"\x00\x00\x00\x1b\x80\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x07\
        TestServer\0",
        )
        .await;
    }

    pub async fn bind_transmitter(&mut self) {
        self.send_and_expect_response(
            b"\x00\x00\x00\x29\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x07\
        esmeid\0password\0type\0\x34\x00\x00\0",
            b"\x00\x00\x00\x1b\x80\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x07\
        TestServer\0",
        )
        .await;
    }

    pub async fn bind_transceiver(&mut self) {
        self.send_and_expect_response(
            b"\x00\x00\x00\x29\x00\x00\x00\x09\x00\x00\x00\x00\x00\x00\x00\x07\
        esmeid\0password\0type\0\x34\x00\x00\0",
            b"\x00\x00\x00\x1b\x80\x00\x00\x09\x00\x00\x00\x00\x00\x00\x00\x07\
        TestServer\0",
        )
        .await;
    }

    pub async fn bind_transceiver_as(&mut self, system_id: &str) {
        let mut bind_pdu = Vec::<u8>::new();
        bind_pdu.extend(b"\x00\x00\x00\x09\x00\x00\x00\x00\x00\x00\x00\x07");
        bind_pdu.extend(system_id.as_bytes());
        bind_pdu.extend(b"\0password\0type\0\x34\x00\x00\0");

        let len: u32 = (bind_pdu.len() + 4) as u32;
        let len_bytes: [u8; 4] = len.to_be_bytes();
        bind_pdu.insert(0, len_bytes[3]);
        bind_pdu.insert(0, len_bytes[2]);
        bind_pdu.insert(0, len_bytes[1]);
        bind_pdu.insert(0, len_bytes[0]);

        self.send_and_expect_response(
            &bind_pdu,
            b"\x00\x00\x00\x1b\x80\x00\x00\x09\x00\x00\x00\x00\x00\x00\x00\x07\
        TestServer\0",
        )
        .await;
    }

    pub async fn into_bound_transmitter(mut self) -> Self {
        self.bind_transmitter().await;
        self
    }

    async fn send_exp(&mut self, input: &[u8], expected_output: &[u8]) {
        self.stream.write(input).await.unwrap();
        self.expect_to_receive(expected_output).await;
    }

    pub async fn send_and_expect_response(
        &mut self,
        input: &[u8],
        expected_output: &[u8],
    ) {
        self.send_exp(input, expected_output).await;
    }

    pub async fn send_and_expect_error_response(
        &mut self,
        input: &[u8],
        expected_output: &[u8],
        expected_error: &str,
    ) {
        self.send_exp(input, expected_output).await;

        // Since this is an error, server should drop the connection
        let resp = self.stream.read_u8().await.unwrap_err();
        assert_eq!(&resp.to_string(), expected_error);
    }

    pub async fn expect_to_receive(&mut self, expected_output: &[u8]) {
        let resp = self.read_n(expected_output.len()).await;
        assert_eq!(bytes_as_string(&resp), bytes_as_string(expected_output));
    }

    pub async fn write_str(&mut self, output: &str) -> AsyncResult<()> {
        self.stream.write_all(output.as_bytes()).await?;
        Ok(())
    }

    pub async fn read_string(&mut self) -> AsyncResult<String> {
        let mut buf = vec![0; 1024];
        let n = self.stream.read(&mut buf).await?;
        let ret = String::from_utf8_lossy(&buf[..n]).to_string();
        Ok(ret)
    }

    pub async fn read_n_maybe(
        &mut self,
        n: usize,
    ) -> Result<Vec<u8>, io::Error> {
        let mut bytes: Vec<u8> = Vec::with_capacity(n);

        while bytes.len() < n {
            bytes.push(self.stream.read_u8().await?);
        }
        Ok(bytes)
    }

    pub async fn read_n(&mut self, n: usize) -> Vec<u8> {
        self.read_n_maybe(n)
            .await
            .unwrap_or_else(|e| match e.kind() {
                io::ErrorKind::UnexpectedEof => {
                    panic!("Error: Not enough bytes to read.")
                }
                _ => panic!("Error while reading: {}", e),
            })
    }
}

#[allow(dead_code)]
pub fn bytes_as_string(arr: &[u8]) -> String {
    arr.iter()
        .map(|x| format!("{:>02x}", x))
        .collect::<Vec<String>>()
        .join("")
}
