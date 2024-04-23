use log::*;
use smpp_pdu::pdu::{
    BindReceiverRespPdu, BindTransceiverRespPdu, BindTransmitterRespPdu,
    EnquireLinkRespPdu, GenericNackPdu, Pdu, PduBody, PduParseError, PduStatus,
    SubmitSmPdu, SubmitSmRespPdu,
};
use std::collections::HashMap;
use std::error;
use std::fmt::{Display, Formatter};
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{Mutex, Semaphore, TryAcquireError};
use tokio::time::sleep;

use crate::async_result::AsyncResult;
use crate::message_unique_key::MessageUniqueKey;
use crate::smpp_connection::{EsmeId, SmppConnection};
use crate::smsc::{SmscConfig, SmscLogic};

pub fn run<L: SmscLogic + Send + Sync + 'static>(
    config: SmscConfig,
    smsc_logic: L,
) -> AsyncResult<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let smsc = Smsc::start(config, smsc_logic).await?;
        loop {
            if let Err(e) = smsc.lock().await.stopped().await {
                return Err(e);
            }
            sleep(Duration::from_millis(100)).await;
            // TODO: notify instead of poll?
        }
    })
}

pub struct Smsc {
    connections: HashMap<EsmeId, Arc<SmppConnection>>,
    messages: HashMap<MessageUniqueKey, EsmeId>,
}

impl Smsc {
    /// Bind to a TCP socket, and return an object that manages
    /// the list of connected clients.  Spawns a task that deals
    /// with incoming connections, which itself spawns a further
    /// new task each time someone connects.
    pub async fn start<L: SmscLogic + Send + Sync + 'static>(
        smsc_config: SmscConfig,
        smsc_logic: L,
    ) -> AsyncResult<Arc<Mutex<Self>>> {
        info!("Starting SMSC");

        let smsc = Smsc {
            connections: HashMap::new(),
            messages: HashMap::new(),
        };
        let smsc = Arc::new(Mutex::new(smsc));

        let listener = TcpListener::bind(&smsc_config.bind_address).await?;
        info!("Bound on {}", &smsc_config.bind_address);

        // Spawn off a task that deals with incoming connections
        tokio::spawn(listen_loop(
            listener,
            Arc::clone(&smsc),
            smsc_config,
            smsc_logic,
        ));

        Ok(smsc)
    }

    async fn stopped(&self) -> AsyncResult<()> {
        // TODO: check whether we are stopped and return an error if so
        Ok(())
    }

    pub async fn receive_pdu(
        &mut self,
        namespace_id: &str,
        pdu: Pdu,
    ) -> AsyncResult<()> {
        // Later: Issue#5: consider retrying after a delay if unable to match DR
        // Later: Issue#12: handle MOs
        info!("<= receive_pdu() {:?}", pdu);
        match pdu.body() {
            PduBody::DeliverSm(body) => {
                let k =
                    MessageUniqueKey::from_dr(String::from(namespace_id), body);
                match k {
                    Some(message_unique_key) => {
                        self.receive_pdu_for_message(pdu, message_unique_key)
                            .await
                    }
                    None => {
                        Err("Could not extract message ID from supplied PDU."
                            .into())
                    }
                }
            }
            _ => Err("Unexpected PDU type.  Currently we can only \
                    handle deliver_sm PDUs."
                .into()),
        }
    }

    async fn receive_pdu_for_message(
        &mut self,
        pdu: Pdu,
        message_unique_key: MessageUniqueKey,
    ) -> AsyncResult<()> {
        let conn = self.connection_for_message(message_unique_key).await?;
        // Later: Issue#3: in order to support a window size to the client, we
        // will need to put this PDU into a queue rather than writing
        // it immediately here.
        tokio::spawn(async move {
            // We schedule the write here, as a sort-of 1-message queue,
            // so we return immediately, and the IO is done later.
            conn.write_pdu(&pdu).await.map_err(
                |e| error!("Failed to send PDU to client: {}", e), // TODO: give information about the client here
            )
        });
        Ok(())
    }

    pub fn add_connection(&mut self, connection: Arc<SmppConnection>) {
        if let Some(esme_id) = connection.bound_esme_id() {
            self.connections.insert(esme_id, connection);
        } else {
            error!(
                "Failed to add connection {} because it is not bound!",
                connection.socket_addr
            );
        }
    }

    pub async fn remove_connection(
        &mut self,
        connection: &Arc<SmppConnection>,
    ) {
        connection.disconnect().await;
        if let Some(esme_id) = connection.bound_esme_id() {
            self.connections.remove(&esme_id);
        }
    }

    fn add_message(
        &mut self,
        message_unique_key: MessageUniqueKey,
        esme_id: EsmeId,
    ) {
        // Later: Issue#14: delete old entries in this map to keep size bounded
        self.messages.insert(message_unique_key, esme_id);
    }

    async fn connection_for_message(
        &mut self,
        message_unique_key: MessageUniqueKey,
    ) -> AsyncResult<Arc<SmppConnection>> {
        if let Some(esme_id) = self.messages.get(&message_unique_key) {
            if let Some(connection) = self.connections.get(esme_id) {
                Ok(Arc::clone(connection))
            } else {
                Err(format!(
                    "No client connection found with \
                system_id='{}' system_type='{}'.",
                    esme_id.system_id, esme_id.system_type
                )
                .into())
            }
        } else {
            Err(format!(
                "No record found of message with \
                namespaceId='{}', message_id='{}', destination_addr='{}'",
                message_unique_key.namespace_id,
                message_unique_key.message_id,
                message_unique_key.destination_addr
            )
            .into())
        }
    }
}

/// Listen for clients connecting, and spawn a new task every time one does
async fn listen_loop<L: SmscLogic + Send + Sync + 'static>(
    listener: TcpListener,
    smsc: Arc<Mutex<Smsc>>,
    config: SmscConfig,
    logic: L,
) {
    let sem = Arc::new(Semaphore::new(config.max_open_sockets));
    let logic = Arc::new(Mutex::new(logic));
    loop {
        match listener.accept().await {
            Err(e) => {
                error!("Client connection failed: {}", e);
            }
            Ok((tcp_stream, socket_addr)) => {
                tokio::spawn(process_stream(
                    Arc::clone(&sem),
                    SmppConnection::new(tcp_stream, socket_addr),
                    config.clone(),
                    Arc::clone(&logic),
                    Arc::clone(&smsc),
                ));
            }
        }
    }
}

async fn process_stream<L: SmscLogic + Send + Sync + 'static>(
    sem: Arc<Semaphore>,
    connection: SmppConnection,
    config: SmscConfig,
    logic: Arc<Mutex<L>>,
    smsc: Arc<Mutex<Smsc>>,
) {
    let socket_addr = connection.socket_addr.clone();
    let aqu = sem.try_acquire();
    match aqu {
        Ok(_guard) => {
            info!("Connection {} - opened", socket_addr);
            let result = process(connection, config, logic, smsc).await;
            log_result(result, socket_addr);
        }
        Err(TryAcquireError::NoPermits) => {
            error!(
                "Refused connection {} - too many open sockets",
                connection.socket_addr
            );
        }
        Err(TryAcquireError::Closed) => {
            error!("Unexpected error: semaphore closed");
        }
    }
}

fn log_result(closed_by_us: Result<bool, ProcessError>, addr: SocketAddr) {
    match closed_by_us {
        Ok(true) => {
            info!("Connection {} - closed by us", addr)
        }
        Ok(false) => info!(
            "Connection {} - closed since client closed the socket",
            addr
        ),
        Err(e) => {
            error!("Connection {} - closed due to error: {}", addr, e)
        }
    }
}

#[derive(Debug)]
struct UnexpectedPduType {
    command_id: u32,
    sequence_number: u32,
}

#[derive(Debug)]
enum ProcessError {
    PduParseError(PduParseError),
    UnexpectedPduType(UnexpectedPduType),
    ConnectionNotBoundAsTransmitter,
    IoError(io::Error),
    InternalError(String),
}

impl ProcessError {
    fn new_unexpected_pdu_type(command_id: u32, sequence_number: u32) -> Self {
        ProcessError::UnexpectedPduType(UnexpectedPduType {
            command_id,
            sequence_number,
        })
    }

    fn new_internal_error(message: &str) -> Self {
        ProcessError::InternalError(String::from(message))
    }

    fn new_connection_not_bound_as_transmitter() -> Self {
        ProcessError::ConnectionNotBoundAsTransmitter
    }
}

impl From<PduParseError> for ProcessError {
    fn from(pdu_parse_error: PduParseError) -> Self {
        ProcessError::PduParseError(pdu_parse_error)
    }
}

impl From<io::Error> for ProcessError {
    fn from(io_error: io::Error) -> Self {
        ProcessError::IoError(io_error)
    }
}

impl Display for ProcessError {
    fn fmt(
        &self,
        formatter: &mut Formatter,
    ) -> std::result::Result<(), std::fmt::Error> {
        let s = match self {
            ProcessError::PduParseError(e) => e.to_string(),
            ProcessError::UnexpectedPduType(e) => {
                format!(
                    "Unexpected PDU type \
                    (command_id={:#010X}, sequence_number={:#010X})",
                    e.command_id, e.sequence_number
                )
            }
            ProcessError::ConnectionNotBoundAsTransmitter => String::from(
                "Attempted to transmit over a connection that was not bound \
                as a transmitter!",
            ),
            ProcessError::IoError(e) => e.to_string(),
            ProcessError::InternalError(s) => String::from(s),
        };
        formatter.write_str(&s)
    }
}

impl error::Error for ProcessError {}

async fn process<L: SmscLogic>(
    connection: SmppConnection,
    config: SmscConfig,
    smsc_logic: Arc<Mutex<L>>,
    smsc: Arc<Mutex<Smsc>>,
) -> Result<bool, ProcessError> {
    struct DisconnectGuard {
        smsc: Arc<Mutex<Smsc>>,
        connection: Arc<SmppConnection>,
    }

    impl Drop for DisconnectGuard {
        fn drop(&mut self) {
            futures::executor::block_on(async move {
                self.smsc
                    .lock()
                    .await
                    .remove_connection(&self.connection)
                    .await;
            });
        }
    }

    // Ensure we disconnect connection when we leave this function,
    // even though we are wrapping it in an Arc so it can be accessed
    // from elsewhere.
    let disconnect_guard = DisconnectGuard {
        smsc: Arc::clone(&smsc),
        connection: Arc::new(connection),
    };

    process_loop(
        Arc::clone(&disconnect_guard.connection),
        config,
        smsc_logic,
        smsc,
    )
    .await
}

async fn process_loop<L: SmscLogic>(
    connection: Arc<SmppConnection>,
    config: SmscConfig,
    smsc_logic: Arc<Mutex<L>>,
    smsc: Arc<Mutex<Smsc>>,
) -> Result<bool, ProcessError> {
    loop {
        let pdu = connection.read_pdu().await;
        match pdu {
            Ok(pdu) => {
                if let Some(pdu) = pdu {
                    let sequence_number = pdu.sequence_number.value;
                    match handle_pdu(
                        pdu,
                        Arc::clone(&connection),
                        &config,
                        Arc::clone(&smsc_logic),
                        Arc::clone(&smsc),
                    )
                    .await
                    {
                        Ok(response) => connection.write_pdu(&response).await?,
                        Err(e) => {
                            // Couldn't handle this PDU type.  Send a nack...
                            connection
                                .write_pdu(
                                    &Pdu::new(
                                        PduStatus::ESME_RINVCMDID as u32,
                                        sequence_number,
                                        GenericNackPdu::new_error().into(),
                                    )
                                    .unwrap(),
                                )
                                .await?;
                            // ...and Drop the connection.
                            return Err(e);
                        }
                    }
                } else {
                    // Client closed the connection
                    return Ok(false);
                }
            }
            Err(pdu_parse_error) => {
                // Respond with an error
                let response = handle_pdu_parse_error(&pdu_parse_error);
                connection.write_pdu(&response).await?;

                // Then return the error, so we drop the connection
                return Err(pdu_parse_error.into());
            }
        }
    }
}

fn handle_pdu_parse_error(error: &PduParseError) -> Pdu {
    let sequence_number = error.sequence_number.unwrap_or(1);
    match error.command_id {
        Some(0x00000002) => Pdu::new(
            error.status(),
            sequence_number,
            BindTransmitterRespPdu::new_error().into(),
        )
        .unwrap(),
        // For any PDU type we're not set up for, send generic_nack
        Some(_) => Pdu::new(
            error.status(),
            sequence_number,
            GenericNackPdu::new_error().into(),
        )
        .unwrap(),
        // If we don't even know the PDU type, send generic_nack
        None => Pdu::new(
            error.status(),
            sequence_number,
            GenericNackPdu::new_error().into(),
        )
        .unwrap(),
    }
}

async fn handle_bind_pdu<L: SmscLogic>(
    pdu: Pdu,
    connection: Arc<SmppConnection>,
    config: &SmscConfig,
    smsc_logic: Arc<Mutex<L>>,
    smsc: Arc<Mutex<Smsc>>,
) -> Result<Pdu, ProcessError> {
    let mut command_status = PduStatus::ESME_ROK;

    let (bind_data, ret_body) = match pdu.body() {
        PduBody::BindReceiver(body) => {
            let mut logic = smsc_logic.lock().await;
            Ok((
                body.bind_data(),
                match logic.bind(body.bind_data()).await {
                    Ok(()) => BindReceiverRespPdu::new(&config.system_id)
                        .unwrap()
                        .into(),
                    Err(e) => {
                        command_status = e.into();
                        BindReceiverRespPdu::new_error().into()
                    }
                },
            ))
        }
        PduBody::BindTransceiver(body) => {
            let mut logic = smsc_logic.lock().await;
            Ok((
                body.bind_data(),
                match logic.bind(body.bind_data()).await {
                    Ok(()) => BindTransceiverRespPdu::new(&config.system_id)
                        .unwrap()
                        .into(),
                    Err(e) => {
                        command_status = e.into();
                        BindTransceiverRespPdu::new_error().into()
                    }
                },
            ))
        }
        PduBody::BindTransmitter(body) => {
            let mut logic = smsc_logic.lock().await;
            Ok((
                body.bind_data(),
                match logic.bind(body.bind_data()).await {
                    Ok(()) => BindTransmitterRespPdu::new(&config.system_id)
                        .unwrap()
                        .into(),
                    Err(e) => {
                        command_status = e.into();
                        BindTransmitterRespPdu::new_error().into()
                    }
                },
            ))
        }
        // This function should only be called with a Bind PDU
        _ => Err(ProcessError::new_internal_error(
            "handle_bind_pdu called with non-bind PDU!",
        )),
    }?;

    // If we successfully bound, register this connection so we
    // know to use it when we receive deliver_sm PDUs later
    if command_status == PduStatus::ESME_ROK {
        connection
            .bind(
                bind_data.system_id.value.clone(),
                bind_data.system_type.value.clone(),
            )
            .await;
        // TODO: we only need to know about this connection if it can transmit,
        // right?
        smsc.lock().await.add_connection(connection);
    }

    Pdu::new(command_status as u32, pdu.sequence_number.value, ret_body)
        .map_err(|e| e.into())
}

async fn handle_submit_sm_pdu<L: SmscLogic>(
    body: &SubmitSmPdu,
    sequence_number: u32,
    connection: Arc<SmppConnection>,
    smsc_logic: Arc<Mutex<L>>,
    smsc: Arc<Mutex<Smsc>>,
) -> Result<Pdu, ProcessError> {
    // Later: Issue#15: only do this if bound as a receiver or transceiver -
    // find out using connection.bound_esme_id

    if let Some(esme_id) = connection.bound_esme_id() {
        let mut command_status = PduStatus::ESME_ROK;
        let resp = match smsc_logic
            .lock()
            .await
            .submit_sm(smsc.clone(), body, sequence_number)
            .await
        {
            Ok((resp, message_unique_key)) => {
                smsc.lock().await.add_message(message_unique_key, esme_id);
                resp
            }
            Err(e) => {
                command_status = e.into();
                SubmitSmRespPdu::new_error().into()
            }
        };
        Pdu::new(command_status as u32, sequence_number, resp.into())
            .map_err(|e| e.into())
    } else {
        // Later: Issue#15: check this is not a receiver
        Err(ProcessError::new_connection_not_bound_as_transmitter())
    }
}

async fn handle_pdu<L: SmscLogic>(
    pdu: Pdu,
    connection: Arc<SmppConnection>,
    config: &SmscConfig,
    smsc_logic: Arc<Mutex<L>>,
    smsc: Arc<Mutex<Smsc>>,
) -> Result<Pdu, ProcessError> {
    info!("<= {} {:?}", connection.socket_addr, pdu);
    let sequence_number = pdu.sequence_number.value;
    match pdu.body() {
        PduBody::BindReceiver(_body) => {
            handle_bind_pdu(pdu, connection, config, smsc_logic, smsc)
                .await
                .map_err(|e| e.into())
        }
        PduBody::BindTransmitter(_body) => {
            handle_bind_pdu(pdu, connection, config, smsc_logic, smsc)
                .await
                .map_err(|e| e.into())
        }
        PduBody::BindTransceiver(_body) => {
            handle_bind_pdu(pdu, connection, config, smsc_logic, smsc)
                .await
                .map_err(|e| e.into())
        }

        PduBody::EnquireLink(_body) => Pdu::new(
            PduStatus::ESME_ROK as u32,
            pdu.sequence_number.value,
            EnquireLinkRespPdu::new().into(),
        )
        .map_err(|e| e.into()),

        PduBody::SubmitSm(body) => {
            handle_submit_sm_pdu(
                body,
                sequence_number,
                connection,
                smsc_logic,
                smsc,
            )
            .await
        }
        _ => Err(ProcessError::new_unexpected_pdu_type(
            pdu.command_id().value,
            pdu.sequence_number.value,
        )),
    }
}
