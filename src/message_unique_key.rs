use smpp_pdu::pdu::DeliverSmPdu;

/// A way to identify this message based on the message ID provided by
/// some remove system.
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct MessageUniqueKey {
    /// An identifier for the system which generated the message_id.  For
    /// systems that produce sufficiently unique IDs, this serves as a
    /// namespace to disambiguate these from IDs produced by separate
    /// systems.
    pub namespace_id: String,
    /// The message ID provided by an external system.  In some cases this
    /// may not be very unique, so we require destination_addr as well.
    pub message_id: String,
    /// The destination_addr from the original MT, or the source_addr from
    /// a delivery receipt.
    /// Note: https://smpp.org/SMPP_v3_4_Issue1_2.pdf Section 2.11 states:
    /// In a SMSC delivery receipt:
    ///   "The destination address will be taken from the source address of
    ///   the original short message which generated the delivery receipt."
    pub destination_addr: String,
}

impl MessageUniqueKey {
    pub fn new(
        namespace_id: String,
        message_id: String,
        destination_addr: String,
    ) -> Self {
        Self {
            namespace_id,
            message_id,
            destination_addr,
        }
    }

    pub fn from_dr(namespace_id: String, pdu: &DeliverSmPdu) -> Option<Self> {
        // Use the source_addr from the DR as the destination_addr.  See
        // section 2.11 of https://smpp.org/SMPP_v3_4_Issue1_2.pdf
        let destination_addr = pdu.source_addr();
        let message_id = pdu.extract_receipted_message_id();
        message_id.map(|message_id| Self {
            namespace_id,
            message_id,
            destination_addr,
        })
    }
}
