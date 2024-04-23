//! A minimal SMSC implementation that allows anyone to log in, but responds
//! to all incoming MTs with an error.

use async_trait::async_trait;
use smpp_pdu::pdu::{SubmitSmPdu, SubmitSmRespPdu};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::message_unique_key::MessageUniqueKey;
use crate::smsc::{BindData, BindError, Smsc, SmscLogic, SubmitSmError};

pub struct AllMtsFail {}

impl AllMtsFail {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl SmscLogic for AllMtsFail {
    async fn bind(&mut self, _bind_data: &BindData) -> Result<(), BindError> {
        // Always consider all system_id/password combinations valid
        Ok(())
    }

    async fn submit_sm(
        &mut self,
        _smsc: Arc<Mutex<Smsc>>,
        _pdu: &SubmitSmPdu,
        _sequence_number: u32,
    ) -> Result<(SubmitSmRespPdu, MessageUniqueKey), SubmitSmError> {
        // Respond with error to all MTs
        Err(SubmitSmError::InternalError)
    }
}
