use clap::Clap;

/// Short Message Service Center (SMSC) in Rust
#[derive(Clap, Clone, Debug)]
#[clap(name = "smsc")]
pub struct SmscConfig {
    /// Address to bind on
    #[clap(short, long, default_value = "0.0.0.0:8080", env = "BIND_ADDRESS")]
    pub bind_address: String,

    /// Maximum number of sockets that can be open
    #[clap(short, long, default_value = "100", env = "MAX_OPEN_SOCKETS")]
    pub max_open_sockets: usize,

    /// system_id used as an identifier of the SMSC
    #[clap(short, long, default_value = "rust_smpp", env = "SYSTEM_ID")]
    pub system_id: String,
}
