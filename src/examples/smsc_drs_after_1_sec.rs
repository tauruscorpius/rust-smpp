//! Logic for an SMSC that returns DRs after approximately 1 second's delay
//! and allows you to bind if system_id==password.

use async_trait::async_trait;
use smpp_pdu::pdu::tlvs::{KnownTlvTag, Tlv, Tlvs};
use smpp_pdu::pdu::{
    DeliverEsmClass, DeliverSmPdu, Pdu, SubmitSmPdu, SubmitSmRespPdu,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time;

use crate::message_unique_key::MessageUniqueKey;
use crate::smsc::{BindData, BindError, Smsc, SmscLogic, SubmitSmError};

pub struct DrsAfter1Sec {}

impl DrsAfter1Sec {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl SmscLogic for DrsAfter1Sec {
    async fn bind(&mut self, bind_data: &BindData) -> Result<(), BindError> {
        if bind_data.system_id == bind_data.password {
            Ok(())
        } else {
            Err(BindError::IncorrectPassword)
        }
    }

    async fn submit_sm(
        &mut self,
        smsc: Arc<Mutex<Smsc>>,
        pdu: &SubmitSmPdu,
        sequence_number: u32,
    ) -> Result<(SubmitSmRespPdu, MessageUniqueKey), SubmitSmError> {
        let message_id = "abc"; // TODO
        let deliver_sm = create_deliver_sm(message_id, sequence_number, pdu);
        tokio::spawn(async move {
            time::sleep(Duration::from_secs(1)).await;
            smsc.lock()
                .await
                .receive_pdu("MySupplier", deliver_sm)
                .await
        });
        Ok((
            SubmitSmRespPdu::new(message_id).unwrap().into(),
            MessageUniqueKey::new(
                String::from("MySupplier"),
                String::from(message_id),
                pdu.destination_addr(),
            ),
        ))
    }
}

fn create_deliver_sm(
    message_id: &str,
    sequence_number: u32,
    submit_sm: &SubmitSmPdu,
) -> Pdu {
    Pdu::new(
        0x00,
        sequence_number,
        DeliverSmPdu::new(
            "",
            submit_sm.dest_addr_ton(),
            submit_sm.dest_addr_npi(),
            &submit_sm.destination_addr(),
            submit_sm.source_addr_ton(),
            submit_sm.source_addr_npi(),
            &submit_sm.source_addr(),
            DeliverEsmClass::SmscDeliveryReceipt as u8,
            0x34,
            1,
            "",
            "",
            1,
            0,
            3,
            0,
            "".as_bytes(),
            Tlvs::from(&[Tlv::new(
                KnownTlvTag::receipted_message_id,
                message_id.as_bytes(),
            )]),
        )
        .unwrap()
        .into(),
    )
    .unwrap()
}
