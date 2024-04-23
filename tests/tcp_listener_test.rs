use tokio::io::AsyncWriteExt;

mod test_utils;

use test_utils::{TestClient, TestServer};

const BIND_TRANSMITTER_PDU: &[u8; 0x29] =
    b"\x00\x00\x00\x29\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x02\
    esmeid\0password\0type\0\x34\x00\x00\0";

#[tokio::test]
async fn listens_on_tcp_port() {
    // Given a server
    let server = TestServer::start().await.unwrap();
    // When we connect
    let mut client = TestClient::connect_to(&server).await.unwrap();
    // Then we can write and read to it
    client.stream.write(BIND_TRANSMITTER_PDU).await.unwrap();
    let resp = client.read_string().await.unwrap();
    assert!(resp.len() > 0);
}

#[tokio::test]
async fn stays_connected() {
    // Given a server
    let server = TestServer::start().await.unwrap();
    // When we connect
    let mut client = TestClient::connect_to(&server).await.unwrap();
    // Then we can write and read to it multiple times
    client.stream.write(BIND_TRANSMITTER_PDU).await.unwrap();
    let resp = client.read_string().await.unwrap();
    assert!(resp.len() > 0);
    client.stream.write(BIND_TRANSMITTER_PDU).await.unwrap();
    let resp = client.read_string().await.unwrap();
    assert!(resp.len() > 0);
}

#[tokio::test]
async fn disconnects_clients_when_overloaded() {
    // Given a server that allows <=2 clients
    let server = TestServer::start().await.unwrap();
    // When we connect 3 clients
    let mut client1 = TestClient::connect_to(&server).await.unwrap();
    let mut client2 = TestClient::connect_to(&server).await.unwrap();
    let mut client3 = TestClient::connect_to(&server).await.unwrap();
    client1.stream.write(BIND_TRANSMITTER_PDU).await.unwrap();
    client2.stream.write(BIND_TRANSMITTER_PDU).await.unwrap();
    client3.stream.write(BIND_TRANSMITTER_PDU).await.unwrap();
    let resp1 = client1.read_string().await.unwrap();
    let resp2 = client2.read_string().await.unwrap();
    let resp3_or_err = client3.read_string().await;

    // Then two of them are able to stay connected
    assert!(resp1.len() > 0);
    assert!(resp2.len() > 0);

    // And the third gets immediately disconnected
    let resp3 = resp3_or_err.unwrap_or(String::from(""));
    assert_eq!(resp3, "");
}
