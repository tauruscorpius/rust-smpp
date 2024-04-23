use async_trait::async_trait;
use smpp::message_unique_key::MessageUniqueKey;
use smpp::smsc::{BindData, BindError, Smsc, SmscLogic, SubmitSmError};
use smpp_pdu::pdu::{SubmitSmPdu, SubmitSmRespPdu};
use std::sync::Arc;
use tokio::sync::Mutex;

mod test_utils;

use test_utils::TestSetup;

#[tokio::test]
async fn when_we_receive_bind_transmitter_we_respond_with_resp() {
    // Given a server with a client connected to it
    TestSetup::new()
        .await
        .client
        .send_and_expect_response(
            // When client sends bind_transmitter, sequence_number = 2
            b"\x00\x00\x00\x29\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x02\
        esmeid\0password\0type\0\x34\x00\x00\0",
            // Then server responds bind_transmitter_resp, sequence_number = 2
            b"\x00\x00\x00\x1b\x80\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x02\
        TestServer\0",
        )
        .await;
}

#[tokio::test]
async fn when_we_receive_bind_receiver_we_respond_with_resp() {
    TestSetup::new()
        .await
        .client
        .send_and_expect_response(
            // When client sends bind_receiver, sequence_number = 8
            b"\x00\x00\x00\x29\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x08\
        esmeid\0password\0type\0\x34\x00\x00\0",
            // Then server responds bind_receiver_resp, sequence_number = 8
            b"\x00\x00\x00\x1b\x80\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x08\
        TestServer\0",
        )
        .await;
}

#[tokio::test]
async fn when_we_receive_bind_transceiver_we_respond_with_resp() {
    TestSetup::new()
        .await
        .client
        .send_and_expect_response(
            // When client sends bind_transceiver, sequence_number = 6
            b"\x00\x00\x00\x29\x00\x00\x00\x09\x00\x00\x00\x00\x00\x00\x00\x06\
        esmeid\0password\0type\0\x34\x00\x00\0",
            // Then server responds bind_transceiver_resp, sequence_number = 6
            b"\x00\x00\x00\x1b\x80\x00\x00\x09\x00\x00\x00\x00\x00\x00\x00\x06\
        TestServer\0",
        )
        .await;
}

#[tokio::test]
async fn when_we_bind_with_incorrect_password_we_receive_error() {
    struct PwIsAlwaysWrong {}

    #[async_trait]
    impl SmscLogic for PwIsAlwaysWrong {
        async fn bind(
            &mut self,
            _bind_data: &BindData,
        ) -> Result<(), BindError> {
            Err(BindError::IncorrectPassword)
        }

        async fn submit_sm(
            &mut self,
            _smsc: Arc<Mutex<Smsc>>,
            _pdu: &SubmitSmPdu,
            _sequence_number: u32,
        ) -> Result<(SubmitSmRespPdu, MessageUniqueKey), SubmitSmError>
        {
            panic!("submit_sm not implemented");
        }
    }

    let logic = PwIsAlwaysWrong {};

    let mut t = TestSetup::new_with_logic(logic).await;
    t.client
        .send_and_expect_response(
            // bind_transceiver
            b"\x00\x00\x00\x29\x00\x00\x00\x09\x00\x00\x00\x00\x00\x00\x00\x06\
        esmeid\0password\0type\0\x34\x00\x00\0",
            // command_status=ESME_RINVPASWD
            b"\x00\x00\x00\x10\x80\x00\x00\x09\x00\x00\x00\x0e\x00\x00\x00\x06",
        )
        .await;

    t.new_client().await;
    t.client
        .send_and_expect_response(
            // bind_receiver
            b"\x00\x00\x00\x29\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x06\
        esmeid\0password\0type\0\x34\x00\x00\0",
            // command_status=ESME_RINVPASWD
            b"\x00\x00\x00\x10\x80\x00\x00\x01\x00\x00\x00\x0e\x00\x00\x00\x06",
        )
        .await;

    t.new_client().await;
    t.client
        .send_and_expect_response(
            // bind_transmitter
            b"\x00\x00\x00\x29\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x06\
        esmeid\0password\0type\0\x34\x00\x00\0",
            // command_status=ESME_RINVPASWD
            b"\x00\x00\x00\x10\x80\x00\x00\x02\x00\x00\x00\x0e\x00\x00\x00\x06",
        )
        .await;
}

#[tokio::test]
async fn when_we_receive_enquire_link_we_respond_with_resp() {
    TestSetup::new()
        .await
        .client
        .send_and_expect_response(
            // When client sends enquire_link
            b"\x00\x00\x00\x10\x00\x00\x00\x15\x00\x00\x00\x00\x00\x00\x00\x12",
            // Then server responds enquire_link_resp
            b"\x00\x00\x00\x10\x80\x00\x00\x15\x00\x00\x00\x00\x00\x00\x00\x12",
        )
        .await;
}

#[tokio::test]
async fn when_we_receive_multiple_binds_we_can_keep_track() {
    struct TrackingLogic {
        num_binds: Arc<std::sync::Mutex<u32>>,
    }

    #[async_trait]
    impl SmscLogic for TrackingLogic {
        async fn bind(
            &mut self,
            _bind_data: &BindData,
        ) -> Result<(), BindError> {
            *self.num_binds.lock().unwrap() += 1;
            Ok(())
        }

        async fn submit_sm(
            &mut self,
            _smsc: Arc<Mutex<Smsc>>,
            _pdu: &SubmitSmPdu,
            _sequence_number: u32,
        ) -> Result<(SubmitSmRespPdu, MessageUniqueKey), SubmitSmError>
        {
            panic!("submit_sm not implemented");
        }
    }

    let num_binds = Arc::new(std::sync::Mutex::new(0));
    let logic = TrackingLogic {
        num_binds: Arc::clone(&num_binds),
    };

    let mut t = TestSetup::new_with_logic(logic).await;
    t.client
        .send_and_expect_response(
            b"\x00\x00\x00\x29\x00\x00\x00\x09\x00\x00\x00\x00\x00\x00\x00\x06\
        esmeid\0password\0type\0\x34\x00\x00\0",
            b"\x00\x00\x00\x1b\x80\x00\x00\x09\x00\x00\x00\x00\x00\x00\x00\x06\
        TestServer\0",
        )
        .await;

    t.new_client().await;
    t.client
        .send_and_expect_response(
            b"\x00\x00\x00\x29\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x06\
        esmeid\0password\0type\0\x34\x00\x00\0",
            b"\x00\x00\x00\x1b\x80\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x06\
        TestServer\0",
        )
        .await;

    t.new_client().await;
    t.client
        .send_and_expect_response(
            b"\x00\x00\x00\x29\x00\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x06\
        esmeid\0password\0type\0\x34\x00\x00\0",
            b"\x00\x00\x00\x1b\x80\x00\x00\x02\x00\x00\x00\x00\x00\x00\x00\x06\
        TestServer\0",
        )
        .await;

    assert_eq!(*num_binds.lock().unwrap(), 3);
}

// Later: Issue#12: return MO
// Later: Issue#9: client app + system test that allows to compare w CloudHopper
// Later: Issue#8: smpp session states (spec 2.2)
// Later: Issue#2: sc_interface_version TLV in bind response
// Later: Issue#10: Check interface versions in binds and responses, submit_sm
// Later: Issue#11: all PDU types and formats
