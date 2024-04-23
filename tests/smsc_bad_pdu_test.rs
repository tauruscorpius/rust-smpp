use std::io;
use std::iter;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

mod test_utils;

use test_utils::{bytes_as_string, TestClient, TestServer, TestSetup};

#[tokio::test]
async fn when_we_receive_a_bad_pdu_we_respond_with_failure_resp_pdu() {
    TestSetup::new()
        .await
        .client
        .send_and_expect_error_response(
            b"\x00\x00\x00\x29\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x14\
        e\xf0\x9f\x92\xa9d\0password\0type\0\x34\x00\x00\0",
            //  ^^^^ non-ascii
            b"\x00\x00\x00\x10\x80\x00\x00\x02\x00\x00\x00\x08\x00\x00\x00\x14",
            //                               system error ^^^^        seq ^^^^
            // Note: no body part because this is an error response
            "unexpected end of file",
        )
        .await;
}

#[tokio::test]
async fn when_client_disconnects_within_pdu_we_continue_accepting_new_connections(
) {
    const PDU: &[u8; 0x11] =
        b"\x00\x00\x00\x29\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x14e";

    // Given an SMSC
    let server = TestServer::start().await.unwrap();
    // When ESME sends partial data then disconnects
    let mut client1 = TestClient::connect_to(&server).await.unwrap();
    client1.stream.write(PDU).await.unwrap();
    client1.stream.shutdown().await.unwrap();

    // Another client is free to connect afterwards
    TestClient::connect_to(&server).await.unwrap();
}

#[tokio::test]
async fn when_sent_bad_pdu_header_we_respond_generic_nack() {
    TestSetup::new()
        .await
        .client
        .send_and_expect_error_response(
            b"\x00\x00\x00\x01",
            // length is 1! ^^
            b"\x00\x00\x00\x10\x80\x00\x00\x00\x00\x00\x00\x02\x00\x00\x00\x01",
            //     generic_nack ^^^^            ^^^ invalid cmd len   seq ^^^^
            "unexpected end of file",
        )
        .await;
}

#[tokio::test]
async fn when_we_receive_wrong_type_of_pdu_we_respond_generic_nack() {
    TestSetup::new()
        .await
        .client
        .send_and_expect_error_response(
            b"\x00\x00\x00\x1b\x80\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x02\
        TestServer\0",
            // bind_transmitter_resp ^^^^^^^^^^^^^ - doesn't make sense
            b"\x00\x00\x00\x10\x80\x00\x00\x00\x00\x00\x00\x03\x00\x00\x00\x02",
            //       generic_nack ^^^^          invalid cmdid ^^^^        seq ^^^^
            "unexpected end of file",
        )
        .await;
}

#[tokio::test]
async fn when_we_receive_nontlv_pdu_with_too_long_length_return_an_error() {
    const PDU: &[u8; 0x29] =
        b"\x00\x00\xff\xff\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x02\
        esmeid\0password\0type\0\x34\x00\x00\0";
    // length longer than content

    let many_bytes: Vec<u8> = PDU
        .iter()
        .copied()
        .chain(iter::repeat(0x00))
        .take(100_000)
        .collect();

    TestSetup::new()
        .await
        .client
        .send_and_expect_error_response(
            &many_bytes,
            b"\x00\x00\x00\x10\x80\x00\x00\x02\x00\x00\x00\x02\x00\x00\x00\x02",
            //      bind_transmitter_resp ^^^^              ^^ cmd len invalid
            "Connection reset by peer (os error 104)",
        )
        .await;
}

#[tokio::test]
async fn when_we_receive_a_pdu_with_very_long_length_we_respond_generic_nack() {
    const PDU: &[u8; 0x1b] =
        b"\x00\xff\xff\xff\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x02\
        e\0pd\0t\0\x34\x00\x00\0";
    // very long length

    const RESP: &[u8; 0x10] =
        b"\x00\x00\x00\x10\x80\x00\x00\x00\x00\x00\x00\x02\x00\x00\x00\x01";
    //       generic_nack ^^^^        cmd len invalid ^^^^        seq ^^^^

    // Note: we don't provide the correct sequence number here: we could, but
    // we would have to read the PDU header before we notice the invalid
    // PDU length.  Since a huge length is likely to indicate a catastrophic
    // error, or malicious traffic, we are not too bothered.

    let many_bytes: Vec<u8> = PDU
        .iter()
        .copied()
        .chain(iter::repeat(0x00))
        .take(0x00ffffff)
        .collect();

    // Given an SMSC
    let server = TestServer::start().await.unwrap();
    // When we send a huge PDU with huge length
    let mut client = TestClient::connect_to(&server).await.unwrap();
    client.stream.write(&many_bytes).await.unwrap();

    // Then SMSC either ...
    let resp = client.read_n_maybe(RESP.len()).await;

    match resp {
        // responds with an error then drops the connection
        Ok(resp) => {
            assert_eq!(bytes_as_string(&resp), bytes_as_string(RESP));
            assert_eq!(
                client.stream.read_u8().await.unwrap_err().kind(),
                io::ErrorKind::ConnectionReset
            );
        }
        // or drops the connection immediately
        Err(e) => {
            assert_eq!(e.kind(), io::ErrorKind::ConnectionReset);
        }
    }
}

#[tokio::test]
async fn when_receive_pdu_with_short_length_but_long_string_we_respond_with_error(
) {
    const BEGIN: &[u8; 0x11] =
        b"\x00\x00\x00\x1b\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x02e";
    const END: &[u8; 0x0a] = b"\0pd\0t\0\x34\x00\x00\0";

    // Our PDU will contain 100,000 letter 'e's within a COctetString
    let mut many_bytes: Vec<u8> = vec![];
    many_bytes.extend(BEGIN.iter());
    many_bytes.extend(iter::repeat('e' as u8).take(100_000));
    many_bytes.extend(END.iter());

    TestSetup::new()
        .await
        .client
        .send_and_expect_error_response(
            &many_bytes,
            b"\x00\x00\x00\x10\x80\x00\x00\x02\x00\x00\x00\x08\x00\x00\x00\x02",
            //      bind_transmitter_resp ^^^^ system error ^^
            "Connection reset by peer (os error 104)",
        )
        .await;
}

#[tokio::test]
async fn when_we_receive_invalid_pdu_type_we_respond_with_error() {
    TestSetup::new()
        .await
        .client
        .send_and_expect_error_response(
            b"\x00\x00\x00\x10\xff\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x22",
            //    this is invalid! ^^^^^^^^^^^^^^^                    seq ^^^^
            b"\x00\x00\x00\x10\x80\x00\x00\x00\x00\x00\x00\x03\x00\x00\x00\x22",
            //   generic_nack ^^^^          invalid cmdid ^^^^        seq ^^^^
            "unexpected end of file",
        )
        .await;
}
