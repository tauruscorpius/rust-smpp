use async_trait::async_trait;
use smpp::message_unique_key::MessageUniqueKey;
use smpp::smsc::{BindData, BindError, Smsc, SmscLogic, SubmitSmError};
use smpp_pdu::pdu::tlvs::{KnownTlvTag, Tlv, Tlvs};
use smpp_pdu::pdu::{
    DeliverEsmClass, DeliverSmPdu, Pdu, SubmitEsmClass, SubmitSmPdu,
    SubmitSmRespPdu,
};
use std::sync::Arc;
use tokio::sync::Mutex;

mod test_utils;

use test_utils::{bytes_as_string, TestSetup};

#[tokio::test]
async fn when_we_receive_deliver_sm_for_a_message_we_provide_it_to_client() {
    let msgid = "ab87J";
    let submit_sm = new_submit_sm(0x2f).await;
    let submit_sm_resp = new_submit_sm_resp(0x2f, msgid).await;
    let logic = Logic {
        msgid: String::from(msgid),
    };

    let mut t = TestSetup::new_with_logic(logic).await;
    t.client.bind_transceiver().await;

    t.client
        .send_and_expect_response(&submit_sm, &submit_sm_resp)
        .await;

    let deliver_sm_pdu = new_deliver_sm_pdu(
        format!("id:{} submit date:2103301649", msgid).as_bytes(),
    );
    let mut deliver_sm = Vec::new();
    deliver_sm_pdu.write(&mut deliver_sm).await.unwrap();

    t.server
        .receive_pdu("testsystem", deliver_sm_pdu)
        .await
        .unwrap();

    let resp = t.client.read_n(deliver_sm.len()).await;
    assert_eq!(bytes_as_string(&resp), bytes_as_string(&deliver_sm));
}

#[tokio::test]
async fn when_we_receive_deliver_sm_with_tlv_for_a_message_we_provide_it() {
    let msgid = "ab87J";
    let submit_sm = new_submit_sm(0x2f).await;
    let submit_sm_resp = new_submit_sm_resp(0x2f, msgid).await;
    let logic = Logic {
        msgid: String::from(msgid),
    };

    let mut t = TestSetup::new_with_logic(logic).await;
    t.client.bind_transceiver().await;

    t.client
        .send_and_expect_response(&submit_sm, &submit_sm_resp)
        .await;

    let deliver_sm_pdu = new_deliver_sm_pdu_with_tlvs(
        "".as_bytes(),
        Tlvs::from(&[Tlv::new(
            KnownTlvTag::receipted_message_id,
            msgid.as_bytes(),
        )]),
    );
    let mut deliver_sm = Vec::new();
    deliver_sm_pdu.write(&mut deliver_sm).await.unwrap();

    t.server
        .receive_pdu("testsystem", deliver_sm_pdu)
        .await
        .unwrap();

    let resp = t.client.read_n(deliver_sm.len()).await;
    assert_eq!(bytes_as_string(&resp), bytes_as_string(&deliver_sm));
}

struct Logic {
    msgid: String,
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
        Ok((
            SubmitSmRespPdu::new(&self.msgid).unwrap(),
            MessageUniqueKey::new(
                String::from("testsystem"),
                self.msgid.clone(),
                pdu.destination_addr(),
            ),
        ))
    }
}

fn new_deliver_sm_pdu(short_message: &[u8]) -> Pdu {
    new_deliver_sm_pdu_with_tlvs(short_message, Tlvs::new())
}

fn new_deliver_sm_pdu_with_tlvs(short_message: &[u8], tlvs: Tlvs) -> Pdu {
    Pdu::new(
        0x00,
        0x6d,
        DeliverSmPdu::new(
            "",
            0,
            0,
            "447777222222",
            0,
            0,
            "MyCompany",
            DeliverEsmClass::SmscDeliveryReceipt as u8,
            0x34,
            1,
            "",
            "",
            1,
            0,
            3,
            0,
            short_message,
            tlvs,
            // Later: Issue#6: enforce meaning of e.g. esm_class
        )
        .unwrap()
        .into(),
    )
    .unwrap()
}

async fn new_submit_sm(sequence_number: u32) -> Vec<u8> {
    let pdu: Pdu = Pdu::new(
        0,
        sequence_number,
        SubmitSmPdu::new(
            "",
            0,
            0,
            "MyCompany",
            0,
            0,
            "447777222222",
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

    let mut ret: Vec<u8> = Vec::new();
    pdu.write(&mut ret).await.unwrap();

    ret
}

async fn new_submit_sm_resp(sequence_number: u32, msgid: &str) -> Vec<u8> {
    let pdu: Pdu = Pdu::new(
        0,
        sequence_number,
        SubmitSmRespPdu::new(msgid).unwrap().into(),
    )
    .unwrap();

    let mut ret: Vec<u8> = Vec::new();
    pdu.write(&mut ret).await.unwrap();

    ret
}

// Later: Issue#5: Retry or fail deliver_sm or submit_sm when don't receive resp
