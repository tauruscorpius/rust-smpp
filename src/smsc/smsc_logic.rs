use async_trait::async_trait;
use smpp_pdu::pdu::data::bind_data::BindData;
use smpp_pdu::pdu::PduStatus;
use smpp_pdu::pdu::{SubmitSmPdu, SubmitSmRespPdu};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::message_unique_key::MessageUniqueKey;
use crate::smsc::Smsc;

pub enum BindError {
    IncorrectPassword,
    InternalError,
}

impl From<BindError> for PduStatus {
    fn from(e: BindError) -> PduStatus {
        match e {
            BindError::IncorrectPassword => PduStatus::ESME_RINVPASWD,
            BindError::InternalError => PduStatus::ESME_RSYSERR,
        }
    }
}

pub enum SubmitSmError {
    InternalError,
}

impl From<SubmitSmError> for PduStatus {
    fn from(e: SubmitSmError) -> PduStatus {
        match e {
            SubmitSmError::InternalError => PduStatus::ESME_RSYSERR,
        }
    }
}

#[async_trait]
pub trait SmscLogic {
    async fn bind(&mut self, bind_data: &BindData) -> Result<(), BindError>;
    async fn submit_sm(
        &mut self,
        smsc: Arc<Mutex<Smsc>>,
        pdu: &SubmitSmPdu,
        sequence_number: u32,
    ) -> Result<(SubmitSmRespPdu, MessageUniqueKey), SubmitSmError>;
}
