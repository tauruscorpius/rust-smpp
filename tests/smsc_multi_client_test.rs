use async_trait::async_trait;
use smpp::message_unique_key::MessageUniqueKey;
use smpp::smsc::{BindData, BindError, Smsc, SmscLogic, SubmitSmError};
use smpp_pdu::pdu::tlvs::Tlvs;
use smpp_pdu::pdu::{
    DeliverEsmClass, DeliverSmPdu, Pdu, SubmitEsmClass, SubmitSmPdu,
    SubmitSmRespPdu,
};
use std::sync::Arc;
use tokio::sync::Mutex;

mod test_utils;

use test_utils::{TestClient, TestServer};

#[tokio::test]
async fn when_multiple_clients_send_mts_we_deliver_drs_to_the_right_one() {
    // 3 clients connect to a server
    let logic = Logic::new(vec![1, 2, 3, 4]);
    let server = TestServer::start_with_logic_and_config(logic, 3)
        .await
        .unwrap();
    let mut client1 = TestClient::connect_to(&server).await.unwrap();
    let mut client2 = TestClient::connect_to(&server).await.unwrap();
    let mut client3 = TestClient::connect_to(&server).await.unwrap();

    client1.bind_transceiver_as("client1").await;
    client2.bind_transceiver_as("client2").await;
    client3.bind_transceiver_as("client3").await;

    // Each client sends an MT
    client1
        .send_and_expect_response(&mt(1).await, &mt_resp(1).await)
        .await;
    client2
        .send_and_expect_response(&mt(2).await, &mt_resp(2).await)
        .await;
    client3
        .send_and_expect_response(&mt(3).await, &mt_resp(3).await)
        .await;
    client2
        .send_and_expect_response(&mt(4).await, &mt_resp(4).await)
        .await;

    // The DR for client3 comes back first
    server
        .receive_pdu("multiclienttestsystem", dr(3))
        .await
        .unwrap();
    // and it received it
    client3.expect_to_receive(&write(dr(3)).await).await;

    // Then the others, and each goes to the client that sent the relevant MT
    server
        .receive_pdu("multiclienttestsystem", dr(1))
        .await
        .unwrap();
    server
        .receive_pdu("multiclienttestsystem", dr(2))
        .await
        .unwrap();
    server
        .receive_pdu("multiclienttestsystem", dr(4))
        .await
        .unwrap();

    // Reading in clients out-of-order is fine
    client2.expect_to_receive(&write(dr(2)).await).await;
    client1.expect_to_receive(&write(dr(1)).await).await;
    client2.expect_to_receive(&write(dr(4)).await).await;
}

#[tokio::test]
async fn when_client_disconnects_and_reconnects_they_can_receive_drs() {
    let logic = Logic::new(vec![1, 2]);
    let server = TestServer::start_with_logic(logic).await.unwrap();
    let mut client2 = TestClient::connect_to(&server).await.unwrap();
    {
        let mut client1 = TestClient::connect_to(&server).await.unwrap();

        client1.bind_transceiver_as("client1").await;
        client2.bind_transceiver_as("client2").await;

        // Each client sends an MT
        client1
            .send_and_expect_response(&mt(1).await, &mt_resp(1).await)
            .await;
        client2
            .send_and_expect_response(&mt(2).await, &mt_resp(2).await)
            .await;

        // Client 1 disconnects because we let it go out of scope here
    }
    // The same person binds again
    let mut client3 = TestClient::connect_to(&server).await.unwrap();
    client3.bind_transceiver_as("client1").await;

    // The DRs come back
    server
        .receive_pdu("multiclienttestsystem", dr(1))
        .await
        .unwrap();
    server
        .receive_pdu("multiclienttestsystem", dr(2))
        .await
        .unwrap();

    // And the clients receive them
    client3.expect_to_receive(&write(dr(1)).await).await;
    client2.expect_to_receive(&write(dr(2)).await).await;
}

struct Logic {
    msgids: Vec<u32>,
}

impl Logic {
    fn new(mut msgids: Vec<u32>) -> Self {
        // We will pop ids off this, so reverse the order
        msgids.reverse();
        Self { msgids }
    }
}

#[async_trait]
impl SmscLogic for Logic {
    async fn bind(&mut self, _bind_data: &BindData) -> Result<(), BindError> {
        Ok(())
    }

    async fn submit_sm(
        &mut self,
        _smsc: Arc<Mutex<Smsc>>,
        pdu: &SubmitSmPdu,
        _sequence_number: u32,
    ) -> Result<(SubmitSmRespPdu, MessageUniqueKey), SubmitSmError> {
        let msgid = self
            .msgids
            .pop()
            .expect("Received more MTs than IDs I was given!");
        Ok((
            SubmitSmRespPdu::new(&msgid.to_string()).unwrap(),
            MessageUniqueKey::new(
                String::from("multiclienttestsystem"),
                msgid.to_string(),
                pdu.destination_addr(),
            ),
        ))
    }
}

fn dr(sequence_number: u32) -> Pdu {
    Pdu::new(
        0x00,
        sequence_number,
        DeliverSmPdu::new(
            "",
            0,
            0,
            "4477711111",
            0,
            0,
            "MyComp",
            DeliverEsmClass::SmscDeliveryReceipt as u8,
            0x34,
            1,
            "",
            "",
            1,
            0,
            3,
            0,
            format!("id:{}", sequence_number).as_bytes(),
            Tlvs::new(),
        )
        .unwrap()
        .into(),
    )
    .unwrap()
}

async fn mt(sequence_number: u32) -> Vec<u8> {
    let pdu: Pdu = Pdu::new(
        0,
        sequence_number,
        SubmitSmPdu::new(
            "",
            0,
            0,
            "MyComp",
            0,
            0,
            "4477711111",
            SubmitEsmClass::Default as u8,
            0x34,
            1,
            "",
            "",
            1,
            0,
            3,
            0,
            b"dr \xffpls",
            Tlvs::new(),
        )
        .unwrap()
        .into(),
    )
    .unwrap();

    write(pdu).await
}

async fn mt_resp(sequence_number: u32) -> Vec<u8> {
    let pdu: Pdu = Pdu::new(
        0,
        sequence_number,
        SubmitSmRespPdu::new(&sequence_number.to_string())
            .unwrap()
            .into(),
    )
    .unwrap();

    write(pdu).await
}

async fn write(pdu: Pdu) -> Vec<u8> {
    let mut ret: Vec<u8> = Vec::new();
    pdu.write(&mut ret).await.unwrap();
    ret
}

// Later: Issue#15: send DR over a receiver connection when bound as transmitter
// Later: Issue#5: drop DRs after some time trying to deliver
