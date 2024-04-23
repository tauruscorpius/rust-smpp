use async_trait::async_trait;
use smpp::message_unique_key::MessageUniqueKey;
use smpp::smsc::{BindData, BindError, Smsc, SmscLogic, SubmitSmError};
use smpp_pdu::pdu::{SubmitSmPdu, SubmitSmRespPdu};
use std::sync::Arc;
use tokio::sync::Mutex;

mod test_utils;

use test_utils::TestSetup;

#[tokio::test]
async fn when_we_receive_submit_sm_we_respond_with_resp() {
    let mut pdu: Vec<u8> = Vec::new();
    pdu.extend(b"\x00\x00\x00\x3d"); //   command_length = 61
    pdu.extend(b"\x00\x00\x00\x04"); //       command_id = submit_sm
    pdu.extend(b"\x00\x00\x00\x00"); //   command_status = NULL
    pdu.extend(b"\x00\x00\x00\x03"); //  sequence_number = 3
    pdu.extend(b"\x00"); //                 service_type = 0
    pdu.extend(b"\x00"); //               source_add_ton = 0
    pdu.extend(b"\x00"); //              source_addr_npi = 0
    pdu.extend(b"447000123123\x00"); //      source_addr
    pdu.extend(b"\x00"); //                 dest_add_ton = 0
    pdu.extend(b"\x00"); //                dest_addr_npi = 0
    pdu.extend(b"447111222222\x00"); // destination_addr
    pdu.extend(b"\x00"); //                    esm_class = 0
    pdu.extend(b"\x01"); //                  protocol_id = 1
    pdu.extend(b"\x01"); //                priority_flag = 1
    pdu.extend(b"\x00"); //       schedule_delivery_time = 0
    pdu.extend(b"\x00"); //              validity_period = 0
    pdu.extend(b"\x01"); //          registered_delivery = 1
    pdu.extend(b"\x00"); //      replace_if_present_flag = 0
    pdu.extend(b"\x03"); //                  data_coding = 3
    pdu.extend(b"\x00"); //            sm_default_msg_id = 0
    pdu.extend(b"\x04"); //                    sm_length = 4
    pdu.extend(b"hihi"); //                short_message = hihi
    assert_eq!(pdu.len(), 0x3d);

    let mut resp: Vec<u8> = Vec::new();
    resp.extend(b"\x00\x00\x00\x1a"); //  command_length = 17
    resp.extend(b"\x80\x00\x00\x04"); //      command_id = submit_sm_resp
    resp.extend(b"\x00\x00\x00\x00"); //  command_status = ESME_ROK
    resp.extend(b"\x00\x00\x00\x03"); // sequence_number = 3
    resp.extend(b"mymessage\x00"); //         message_id = "mymessage"
    assert_eq!(resp.len(), 0x1a);

    struct Logic {}

    #[async_trait]
    impl SmscLogic for Logic {
        async fn bind(
            &mut self,
            _bind_data: &BindData,
        ) -> Result<(), BindError> {
            Ok(())
        }

        async fn submit_sm(
            &mut self,
            _smsc: Arc<Mutex<Smsc>>,
            _pdu: &SubmitSmPdu,
            _sequence_number: u32,
        ) -> Result<(SubmitSmRespPdu, MessageUniqueKey), SubmitSmError>
        {
            let msgid = "mymessage";
            Ok((
                SubmitSmRespPdu::new(msgid).unwrap(),
                MessageUniqueKey::new(
                    String::from("mttest"),
                    String::from(msgid),
                    String::from("dest"),
                ),
            ))
        }
    }

    TestSetup::new_with_logic(Logic {})
        .await
        .client
        .into_bound_transmitter()
        .await
        .send_and_expect_response(&pdu, &resp)
        .await;
}
