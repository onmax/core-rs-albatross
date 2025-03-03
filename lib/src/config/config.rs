#[cfg(any(feature = "rpc-server", feature = "metrics-server"))]
use std::net::IpAddr;
#[cfg(feature = "metrics-server")]
use std::net::SocketAddr;
use std::{
    fmt,
    num::NonZeroU8,
    path::{Path, PathBuf},
    string::ToString,
};

use derive_builder::Builder;
#[cfg(feature = "validator")]
use nimiq_bls::{KeyPair as BlsKeyPair, SecretKey as BlsSecretKey};
#[cfg(feature = "database-storage")]
use nimiq_database::mdbx::MdbxDatabase;
use nimiq_hash::{Blake2bHash, Hash};
#[cfg(feature = "validator")]
use nimiq_keys::{Address, KeyPair, PrivateKey};
#[cfg(feature = "nimiq-mempool")]
use nimiq_mempool::{config::MempoolConfig, filter::MempoolRules};
use nimiq_network_interface::Multiaddr;
use nimiq_network_libp2p::{Keypair as IdentityKeypair, Libp2pKeyPair};
use nimiq_primitives::{networks::NetworkId, policy::Policy};
use nimiq_serde::Deserialize;
#[cfg(feature = "validator")]
use nimiq_utils::key_rng::SecureGenerate;
use nimiq_utils::{file_store::FileStore, Sensitive};
use nimiq_zkp_circuits::DEFAULT_PROVER_KEYS_PATH;
use subtle::ConstantTimeEq;

#[cfg(feature = "database-storage")]
use crate::config::config_file::DatabaseSettings;
#[cfg(any(feature = "rpc-server", feature = "metrics-server"))]
use crate::config::consts;
#[cfg(feature = "metrics-server")]
use crate::config::consts::default_bind;
use crate::{
    config::{
        command_line::CommandLine,
        config_file::{ConfigFile, Seed, TlsSettings},
        paths,
        user_agent::UserAgent,
    },
    error::Error,
};

/// The sync mode
///
/// # ToDo
///
/// * We'll probably have this enum somewhere in the primitives. So this is a placeholder.
///
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum SyncMode {
    /// History nodes: They use HistoryMacroSync + BlockLiveSync
    History,
    /// Full nodes: They use LightMacroSync + StateLiveSync
    Full,
    /// Light nodes: They use LightMacroSync + BlockLiveSync
    Light,
}

impl fmt::Display for SyncMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl Default for SyncMode {
    fn default() -> Self {
        Self::History
    }
}

#[derive(Debug, Clone, Builder)]
#[builder(setter(into))]
/// Client Consensus settings configuration
pub struct ConsensusConfig {
    #[builder(default)]
    /// Sync mode used based upon its client type
    pub sync_mode: SyncMode,
    #[builder(default = "3")]
    /// Minimum number of peers necessary to reach consensus
    pub min_peers: usize,
    #[builder(default = "1")]
    /// Maximum number of epochs that are stored in the client
    pub max_epochs_stored: u32,
    #[builder(default = "10800")]
    /// Minimum distance away, in number of blocks, from the head to switch from state sync to live sync
    pub full_sync_threshold: u32,
    #[builder(setter(custom))]
    /// History indices enabled. Defaults to `true` for history nodes and `false` to full/light nodes.
    pub index_history: bool,
}

impl ConsensusConfigBuilder {
    fn index_history(
        &mut self,
        should_index_history: Option<bool>,
        sync_mode: SyncMode,
    ) -> &mut Self {
        let index_history = should_index_history.unwrap_or(match sync_mode {
            SyncMode::History => true,
            SyncMode::Full | SyncMode::Light => false,
        });

        self.index_history = Some(index_history);
        self
    }
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        ConsensusConfig {
            sync_mode: SyncMode::default(),
            min_peers: 3,
            max_epochs_stored: Policy::MIN_EPOCHS_STORED,
            full_sync_threshold: 10800,
            index_history: true,
        }
    }
}

/// Network config
#[derive(Debug, Clone, Builder, Default)]
#[builder(setter(into))]
pub struct NetworkConfig {
    /// List of addresses this node is going to listen to
    #[builder(default)]
    pub listen_addresses: Vec<Multiaddr>,

    #[builder(default)]
    pub advertised_addresses: Option<Vec<Multiaddr>>,

    /// The user agent is a custom string that is sent during the handshake. Usually it contains
    /// the kind of node, Nimiq version, processor architecture and operating system. This enables
    /// gathering information on which Nimiq versions are being run on the network. A typical
    /// user agent string looks like `core-rs-albatross/0.1.0 (native; linux x86_64)`.
    ///
    /// Default will generate a value from system information, this is recommended.
    ///
    #[builder(default)]
    pub user_agent: UserAgent,

    /// List of seeds addresses.
    #[builder(default)]
    pub seeds: Vec<Seed>,

    /// Optional, TLS configuration for secure WebSocket.
    #[builder(default)]
    pub tls: Option<TlsConfig>,

    /// Optional, desired number of peers for the network to connect to.
    /// The network will always try to maintain this number of connections.
    #[builder(default = "12")]
    pub desired_peer_count: usize,

    /// Optional, max number of peer connections.
    #[builder(default = "4000")]
    pub peer_count_max: usize,

    /// Optional, max number of peer connections per IP address.
    #[builder(default = "20")]
    pub peer_count_per_ip_max: usize,

    /// Optional, max number of peer connections per subnet IP address.
    #[builder(default = "20")]
    pub peer_count_per_subnet_max: usize,

    /// Optional, bool to only accept secure websocket connections.
    #[builder(default)]
    pub only_secure_ws_connections: bool,

    /// Optional, bool to allow connections to loopback addresses.
    #[builder(default)]
    pub allow_loopback_addresses: bool,

    /// Optional, quorum value for the network DHT.
    #[builder(default)]
    pub dht_quorum: Option<NonZeroU8>,
}

/// Configuration for setting TLS for secure WebSocket
#[derive(Debug, Clone, Default)]
pub struct TlsConfig {
    /// Path to a file containing the private key (PEM-encoded ASN.1 in either PKCS#8 or PKCS#1 format).
    pub private_key: String,
    /// Path to a file containing the certificates (in PEM-encoded X.509 format). In this file several certificates
    /// could be added for certificate chaining.
    pub certificates: String,
}

impl From<TlsSettings> for TlsConfig {
    fn from(value: TlsSettings) -> Self {
        Self {
            private_key: value.private_key,
            certificates: value.certificates,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FileStorageConfig {
    /// The parent directory where the database will be stored. The database directory name
    /// is determined by the network ID and consensus type using the `database_name` static
    /// method.
    pub database_parent: PathBuf,

    /// Path to peer key.
    pub peer_key_path: PathBuf,

    /// The key used for the peer key, if the file is not present.
    pub peer_key: Option<Sensitive<String>>,

    /// Path to voting key.
    #[cfg(feature = "validator")]
    pub voting_key_path: Option<PathBuf>,

    // Paths to voting keys.
    #[cfg(feature = "validator")]
    pub voting_key_paths: Option<Vec<PathBuf>>,

    /// The voting key used for the validator, if the file is not present.
    #[cfg(feature = "validator")]
    pub voting_key: Option<Sensitive<String>>,

    /// Path to signing key.
    #[cfg(feature = "validator")]
    pub signing_key_path: Option<PathBuf>,

    /// The signing key used for the validator, if the file is not present.
    #[cfg(feature = "validator")]
    pub signing_key: Option<Sensitive<String>>,

    /// Path to fee key.
    #[cfg(feature = "validator")]
    pub fee_key_path: Option<PathBuf>,

    /// The fee key used for the validator, if the file is not present.
    #[cfg(feature = "validator")]
    pub fee_key: Option<Sensitive<String>>,
}

impl FileStorageConfig {
    /// Create storage config from a directory path.
    ///
    pub fn from_directory<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();
        Self {
            database_parent: path.to_path_buf(),
            peer_key_path: path.join("peer_key.dat"),
            peer_key: None,
            #[cfg(feature = "validator")]
            voting_key_paths: None,
            #[cfg(feature = "validator")]
            voting_key_path: Some(path.join("voting_key.dat")),
            #[cfg(feature = "validator")]
            voting_key: None,
            #[cfg(feature = "validator")]
            fee_key_path: Some(path.join("fee_key.dat")),
            #[cfg(feature = "validator")]
            fee_key: None,
            #[cfg(feature = "validator")]
            signing_key_path: Some(path.join("signing_key.dat")),
            #[cfg(feature = "validator")]
            signing_key: None,
        }
    }

    /// Stores the database in the users home directory, i.e. `$HOME/.nimiq/`. This is the default.
    ///
    pub fn home() -> Self {
        Self::from_directory(paths::home())
    }

    /// Stores the database in `/var/lib/nimiq/`
    pub fn system() -> Self {
        Self::from_directory(paths::system())
    }
}

impl Default for FileStorageConfig {
    fn default() -> Self {
        Self::home()
    }
}

/// Configuration options for the database
#[cfg(feature = "database-storage")]
#[derive(Debug, Clone, Builder, Eq, PartialEq)]
#[builder(setter(into))]
pub struct DatabaseConfig {
    /// Initial database size. Default: 1 TB
    #[builder(default = "1024 * 1024 * 1024 * 1024")]
    size: usize,

    /// Max number of DBs. Recommended: 20
    #[builder(default = "20")]
    max_dbs: u32,

    /// Max number of threads that can open read transactions.
    /// Tokio by default has a maximum of 1 + num cores + 512 (blocking) threads.
    /// Our default value allows for up to 87 cores if tokio's defaults are not changed.
    /// Recommended: 600
    #[builder(default = "600")]
    max_readers: u32,
}
#[cfg(feature = "database-storage")]
impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            // 1 TB
            size: 1024 * 1024 * 1024 * 1024,
            max_dbs: 20,
            max_readers: 600,
        }
    }
}
#[cfg(feature = "database-storage")]
impl From<Option<DatabaseSettings>> for DatabaseConfig {
    fn from(db_settings: Option<DatabaseSettings>) -> Self {
        let default = DatabaseConfig::default();

        if let Some(db_settings) = db_settings {
            Self {
                size: db_settings.size.unwrap_or(default.size),
                max_dbs: db_settings.max_dbs.unwrap_or(default.max_dbs),
                max_readers: db_settings.max_readers.unwrap_or(default.max_readers),
            }
        } else {
            default
        }
    }
}

/// Determines where the database will be stored.
///
/// # ToDo
///
///  * Implement `TryInto<FileLocations>`?
///
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum StorageConfig {
    /// This will store the database in a volatile storage. After the client shuts
    /// down all data will be lost.
    /// This can be used for browser based environments
    ///
    Volatile,

    /// This will store the database and key files at specific paths. This is not available when
    /// compiled to WebAssembly.
    ///
    Filesystem(FileStorageConfig),
}

impl StorageConfig {
    /// Returns the database environment for that storage backend and the given network ID and
    /// consensus type.
    ///
    /// # Arguments
    ///
    /// * `network_id` - The network ID of the database
    /// * `consensus` - The consensus type
    /// * `db_config` - The database configuration
    ///
    /// # Return Value
    ///
    /// Returns a `Result` which is either a `Environment` or a `Error`.
    ///
    #[cfg(feature = "database-storage")]
    pub fn database(
        &self,
        network_id: NetworkId,
        sync_mode: SyncMode,
        db_config: DatabaseConfig,
    ) -> Result<MdbxDatabase, Error> {
        let db_name = format!("{network_id}-{sync_mode}-consensus").to_lowercase();
        self.open_database(db_config, db_name)
    }

    /// Returns the database environment for the pre-genesis database.
    ///
    /// # Arguments
    ///
    /// * `network_id` - The network ID of the database
    /// * `db_config` - The database configuration
    ///
    /// # Return Value
    ///
    /// Returns a `Result` which is either a `Environment` or a `Error`.
    ///
    #[cfg(feature = "database-storage")]
    pub fn pre_genesis_database(
        &self,
        network_id: NetworkId,
        db_config: DatabaseConfig,
    ) -> Result<MdbxDatabase, Error> {
        assert!(
            matches!(self, StorageConfig::Filesystem(_)),
            "Pre-genesis database is only supported with filesystem storage"
        );
        let db_name = format!("{network_id}-pre-genesis").to_lowercase();
        self.open_database(db_config, db_name)
    }

    /// Checks for the existence of the pre-genesis database.
    #[cfg(feature = "database-storage")]
    pub fn has_pre_genesis_database(&self, network_id: NetworkId) -> bool {
        let db_name = format!("{network_id}-pre-genesis").to_lowercase();
        match self {
            StorageConfig::Volatile => false,
            StorageConfig::Filesystem(file_storage) => {
                let db_path = file_storage.database_parent.join(db_name);
                db_path.exists()
            }
        }
    }

    /// Internal helper function to initiate a `MdbxDatabase` with the given `DatabaseConfig`.
    #[cfg(feature = "database-storage")]
    fn open_database(
        &self,
        db_config: DatabaseConfig,
        db_name: String,
    ) -> Result<MdbxDatabase, Error> {
        use nimiq_database::mdbx;

        log::info!("Opening database: {}", db_name);

        let config = mdbx::DatabaseConfig {
            max_tables: Some(db_config.max_dbs as u64),
            size: Some(-1..db_config.size as isize),
            max_readers: Some(db_config.max_readers),
            ..Default::default()
        };

        Ok(match self {
            StorageConfig::Volatile => MdbxDatabase::new_volatile(config)?,
            StorageConfig::Filesystem(file_storage) => {
                let db_path = file_storage.database_parent.join(db_name);
                let db_path = db_path
                    .to_str()
                    .ok_or_else(|| {
                        Error::config_error(format!(
                            "Failed to convert database path to string: {}",
                            db_path.display()
                        ))
                    })?
                    .to_string();
                MdbxDatabase::new(db_path, config)?
            }
        })
    }

    #[cfg(feature = "validator")]
    pub(crate) fn voting_keypairs(&self) -> Result<Vec<BlsKeyPair>, Error> {
        Ok(match self {
            StorageConfig::Volatile => vec![BlsKeyPair::generate_default_csprng()],
            StorageConfig::Filesystem(file_storage) => {
                if let Some(voting_key_paths) = &file_storage.voting_key_paths {
                    if file_storage.voting_key.is_some() {
                        return Err(Error::config_error(
                            "Option voting_key_paths can't be set at the same time as voting_key",
                        ));
                    }
                    let mut keypairs = Vec::new();
                    for key_path in voting_key_paths {
                        let keypair = FileStore::new(key_path).load()?;
                        keypairs.push(keypair);
                    }
                    keypairs
                } else {
                    let key_path = file_storage.voting_key_path.as_ref().ok_or_else(|| {
                        Error::config_error("No path for validator key specified")
                    })?;
                    let key_path = key_path
                        .to_str()
                        .ok_or_else(|| {
                            Error::config_error(format!(
                                "Failed to convert path of validator key to string: {}",
                                key_path.display()
                            ))
                        })?
                        .to_string();

                    vec![FileStore::new(key_path).load_or_store(|| {
                        if let Some(key) = file_storage.voting_key.as_ref() {
                            // TODO: handle errors
                            let secret_key =
                                BlsSecretKey::deserialize_from_vec(&hex::decode(key).unwrap())
                                    .unwrap();
                            secret_key.into()
                        } else {
                            BlsKeyPair::generate_default_csprng()
                        }
                    })?]
                }
            }
        })
    }

    #[cfg(feature = "validator")]
    pub(crate) fn fee_keypair(&self) -> Result<KeyPair, Error> {
        Ok(match self {
            StorageConfig::Volatile => KeyPair::generate_default_csprng(),
            StorageConfig::Filesystem(file_storage) => {
                let key_path = file_storage
                    .fee_key_path
                    .as_ref()
                    .ok_or_else(|| Error::config_error("No path for fee key specified"))?;
                let key_path = key_path
                    .to_str()
                    .ok_or_else(|| {
                        Error::config_error(format!(
                            "Failed to convert path of fee key to string: {}",
                            key_path.display()
                        ))
                    })?
                    .to_string();

                FileStore::new(key_path).load_or_store(|| {
                    if let Some(key) = file_storage.fee_key.as_ref() {
                        // TODO: handle errors
                        KeyPair::from(
                            PrivateKey::deserialize_from_vec(&hex::decode(key).unwrap()).unwrap(),
                        )
                    } else {
                        KeyPair::generate_default_csprng()
                    }
                })?
            }
        })
    }

    #[cfg(feature = "validator")]
    pub(crate) fn signing_keypair(&self) -> Result<KeyPair, Error> {
        Ok(match self {
            StorageConfig::Volatile => KeyPair::generate_default_csprng(),
            StorageConfig::Filesystem(file_storage) => {
                let key_path = file_storage
                    .signing_key_path
                    .as_ref()
                    .ok_or_else(|| Error::config_error("No path for warm key specified"))?;
                let key_path = key_path
                    .to_str()
                    .ok_or_else(|| {
                        Error::config_error(format!(
                            "Failed to convert path of warm key to string: {}",
                            key_path.display()
                        ))
                    })?
                    .to_string();

                FileStore::new(key_path).load_or_store(|| {
                    if let Some(key) = file_storage.signing_key.as_ref() {
                        // TODO: handle errors
                        KeyPair::from(
                            PrivateKey::deserialize_from_vec(&hex::decode(key).unwrap()).unwrap(),
                        )
                    } else {
                        KeyPair::generate_default_csprng()
                    }
                })?
            }
        })
    }

    pub(crate) fn identity_keypair(&self) -> Result<IdentityKeypair, Error> {
        match self {
            StorageConfig::Volatile => Ok(IdentityKeypair::generate_ed25519()),
            StorageConfig::Filesystem(file_storage) => {
                Ok(FileStore::new(&file_storage.peer_key_path)
                    .load_or_store(|| {
                        if let Some(key) = file_storage.peer_key.as_ref() {
                            // TODO: handle errors
                            Libp2pKeyPair::deserialize_from_vec(&hex::decode(key).unwrap()).unwrap()
                        } else {
                            Libp2pKeyPair(IdentityKeypair::generate_ed25519())
                        }
                    })?
                    .0)
            }
        }
    }
}

impl From<FileStorageConfig> for StorageConfig {
    fn from(config: FileStorageConfig) -> Self {
        StorageConfig::Filesystem(config)
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        FileStorageConfig::default().into()
    }
}

#[cfg(feature = "validator")]
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ValidatorConfig {
    /// The validator address.
    pub validator_address: Address,

    pub dht_fallback_url: Option<url::Url>,

    /// Config if the validator automatically reactivates itself.
    pub automatic_reactivate: bool,
}

/// Credentials for JSON RPC server, metrics server or websocket RPC server
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credentials {
    /// Username.
    pub username: String,
    /// Hash of the password.
    pub password_hash: Sensitive<Blake2bHash>,
}

impl Credentials {
    pub fn new<U: AsRef<str>, P: AsRef<str>>(username: U, password: P) -> Self {
        Self {
            username: username.as_ref().to_owned(),
            password_hash: Sensitive(password.as_ref().hash()),
        }
    }

    pub fn check<U: AsRef<str>, P: AsRef<str>>(&self, username: U, password: P) -> bool {
        (self.username.as_bytes().ct_eq(username.as_ref().as_bytes())
            & self.password_hash.0.ct_eq(&password.as_ref().hash()))
        .into()
    }
}

#[cfg(feature = "rpc-server")]
#[derive(Builder, Clone, Debug)]
#[builder(setter(into))]
pub struct RpcServerConfig {
    /// Bind the RPC server to the specified IP address.
    ///
    /// Default: `127.0.0.1`
    ///
    #[builder(setter(strip_option))]
    pub bind_to: Option<IpAddr>,

    /// Bind the server to the specified port.
    ///
    /// Default: `8648`
    ///
    #[builder(default = "consts::RPC_DEFAULT_PORT")]
    pub port: u16,

    #[builder(setter(strip_option))]
    pub cors_domains: Option<Vec<String>>,

    /// If specified, only allow connections from these IP addresses
    ///
    #[builder(setter(strip_option))]
    pub allow_ips: Option<Vec<IpAddr>>,

    /// If specified, only allow these RPC methods
    ///
    #[builder(setter(strip_option))]
    pub allowed_methods: Option<Vec<String>>,

    /// If specified, require HTTP basic auth with these credentials
    #[builder(setter(strip_option))]
    pub credentials: Option<Credentials>,
}

#[cfg(feature = "metrics-server")]
#[derive(Builder, Clone, Debug)]
#[builder(setter(into))]
pub struct MetricsServerConfig {
    /// Bind the server to the specified ip and port.
    ///
    /// Default: `127.0.0.1:9100`
    ///
    pub addr: SocketAddr,

    /// If specified, require HTTP basic auth with these credentials
    #[builder(setter(strip_option))]
    pub credentials: Option<Credentials>,
}

/// Client configuration
///
/// # ToDo
///
/// * Make this implement `IntoFuture<Item=Client, Err=Error>` so you can just do
///   `tokio::spawn(config.and_then(|client| [...]));`
#[derive(Clone, Debug, Builder, Default)]
#[builder(setter(into), build_fn(private, name = "build_internal"))]
pub struct ClientConfig {
    /// Network config
    #[builder(default)]
    pub network: NetworkConfig,

    /// Consensus config
    #[builder(default)]
    pub consensus: ConsensusConfig,

    /// The Nimiq network the client should connect to. Usually this should be either `Test` or
    /// `Main` for the Nimiq 1.0 networks. For Albatross there is `MainAlbatross`, `TestAlbatross`
    /// and `DevAlbatross` available.
    ///
    /// Default is `MainAlbatross`
    #[builder(default = "NetworkId::MainAlbatross")]
    pub network_id: NetworkId,

    /// Determines where the database is stored.
    ///
    #[builder(default)]
    pub storage: StorageConfig,

    /// Database-specific configuration
    ///
    #[cfg(feature = "database-storage")]
    #[builder(default)]
    pub database: DatabaseConfig,

    /// The mempool filter rules
    ///
    #[cfg(feature = "nimiq-mempool")]
    #[builder(default, setter(custom))]
    pub mempool: MempoolConfig,

    /// The optional validator configuration
    ///
    #[cfg(feature = "validator")]
    #[builder(default)]
    pub validator: Option<ValidatorConfig>,

    /// The optional zk prover configuration
    ///
    #[builder(default)]
    pub zk_prover: Option<ZKProverConfig>,

    /// The optional rpc-server configuration
    ///
    #[cfg(feature = "rpc-server")]
    #[builder(default)]
    pub rpc_server: Option<RpcServerConfig>,

    #[cfg(feature = "metrics-server")]
    #[builder(default)]
    pub metrics_server: Option<MetricsServerConfig>,
}

impl ClientConfig {
    /// Creates a new builder object for the client configuration.
    ///
    pub fn builder() -> ClientConfigBuilder {
        ClientConfigBuilder::default()
    }
}

impl ClientConfigBuilder {
    /// Build a finished config object from the builder
    ///
    pub fn build(&self) -> Result<ClientConfig, Error> {
        // NOTE: We rename the generated builder and make it private to map the error from a plain
        // `String` to an actual Error.
        // We could also put some validation here.

        self.build_internal()
            .map_err(|e| Error::config_error(e.to_string()))
    }

    /// Sets the network ID to the Albatross DevNet
    pub fn dev(&mut self) -> &mut Self {
        self.network_id(NetworkId::DevAlbatross)
    }

    /// Sets the network ID to the Albatross TestNet
    pub fn test(&mut self) -> &mut Self {
        self.network_id(NetworkId::TestAlbatross)
    }

    /// Sets the network ID to the Albatross MainNet
    pub fn main(&mut self) -> &mut Self {
        self.network_id(NetworkId::MainAlbatross)
    }

    /// Configuration for Light Client
    pub fn light(&mut self) -> &mut Self {
        let consensus_config = ConsensusConfig {
            sync_mode: SyncMode::Light,
            ..Default::default()
        };
        self.consensus(consensus_config)
    }

    /// Configures the storage to be volatile. All data will be lost after shutdown of the client.
    pub fn volatile(&mut self) -> &mut Self {
        self.storage = Some(StorageConfig::Volatile);
        self
    }

    #[cfg(feature = "nimiq-mempool")]
    /// Sets the mempool filter rules
    pub fn mempool(
        &mut self,
        size_limit: usize,
        control_size_limit: usize,
        filter_rules: MempoolRules,
        filter_limit: usize,
    ) -> &mut Self {
        self.mempool = Some(MempoolConfig {
            size_limit,
            control_size_limit,
            filter_rules,
            filter_limit,
        });
        self
    }

    /// Applies settings from a configuration file
    pub fn config_file(&mut self, config_file: &ConfigFile) -> Result<&mut Self, Error> {
        // TODO: if the config field of `listen_addresses` is empty, we should at least add `/ip4/127.0.0.1/...`
        self.network(NetworkConfig {
            listen_addresses: config_file
                .network
                .listen_addresses
                .iter()
                .map(|addr| addr.parse())
                .collect::<Result<Vec<Multiaddr>, _>>()?,

            advertised_addresses: if let Some(advertised_addresses) =
                &config_file.network.advertised_addresses
            {
                Some(
                    advertised_addresses
                        .iter()
                        .map(|addr| addr.parse())
                        .collect::<Result<Vec<Multiaddr>, _>>()?,
                )
            } else {
                None
            },

            user_agent: config_file
                .network
                .user_agent
                .as_ref()
                .map(|ua| UserAgent::from(ua.to_owned()))
                .unwrap_or_default(),

            seeds: config_file.network.seed_nodes.clone(),

            desired_peer_count: config_file.network.desired_peer_count,

            peer_count_max: config_file.network.peer_count_max,
            peer_count_per_ip_max: config_file.network.peer_count_per_ip_max,
            peer_count_per_subnet_max: config_file.network.peer_count_per_subnet_max,

            tls: config_file.network.tls.as_ref().map(|s| s.clone().into()),
            only_secure_ws_connections: false,
            allow_loopback_addresses: config_file.network.allow_loopback_addresses,
            dht_quorum: config_file.network.dht_quorum,
        });

        // Configure consensus
        let mut consensus = ConsensusConfigBuilder::default()
            .sync_mode(config_file.consensus.sync_mode)
            .index_history(
                config_file.consensus.index_history,
                config_file.consensus.sync_mode.into(),
            )
            .max_epochs_stored(config_file.consensus.max_epochs_stored as u32)
            .build()
            .unwrap();
        if let Some(min_peers) = config_file.consensus.min_peers {
            consensus.min_peers = min_peers;
        }
        if let Some(full_sync_threshold) = config_file.consensus.full_sync_threshold {
            consensus.full_sync_threshold = full_sync_threshold;
        }
        self.consensus(consensus);

        // Configure network
        if let Some(network) = config_file.consensus.network {
            self.network_id(network);
        }

        // Configure storage config.
        let mut file_storage = FileStorageConfig::default();
        if let Some(db_config_file) = &config_file.database {
            if let Some(path) = db_config_file.path.as_ref() {
                file_storage.database_parent = PathBuf::from(path);
            }
        }
        if let Some(key_path) = config_file.network.peer_key_file.as_ref() {
            file_storage.peer_key_path = PathBuf::from(key_path);
        }
        if let Some(key) = config_file.network.peer_key.as_ref() {
            file_storage.peer_key = Some(key.to_owned());
        }
        #[cfg(feature = "validator")]
        if let Some(validator_config) = config_file.validator.as_ref() {
            self.validator(ValidatorConfig {
                validator_address: Address::from_any_str(&validator_config.validator_address)?,
                dht_fallback_url: validator_config.dht_fallback_url.clone(),
                automatic_reactivate: validator_config.automatic_reactivate,
            });

            if let Some(key_paths) = &validator_config.voting_key_files {
                file_storage.voting_key_paths = Some(key_paths.iter().map(PathBuf::from).collect())
            }
            if let Some(key_path) = &validator_config.voting_key_file {
                file_storage.voting_key_path = Some(PathBuf::from(key_path));
            }
            if let Some(key) = &validator_config.voting_key {
                file_storage.voting_key = Some(key.to_owned());
            }
            if let Some(key_path) = &validator_config.fee_key_file {
                file_storage.fee_key_path = Some(PathBuf::from(key_path));
            }
            if let Some(key) = &validator_config.fee_key {
                file_storage.fee_key = Some(key.to_owned());
            }
            if let Some(key_path) = &validator_config.signing_key_file {
                file_storage.signing_key_path = Some(PathBuf::from(key_path));
            }
            if let Some(key) = &validator_config.signing_key {
                file_storage.signing_key = Some(key.to_owned());
            }
        }
        self.storage = Some(file_storage.into());

        // Configure database
        #[cfg(feature = "database-storage")]
        self.database(config_file.database.clone());

        // Configure the zk prover
        if let Some(zkp_settings) = config_file.zk_prover.as_ref() {
            let prover_keys_path = zkp_settings
                .prover_keys_path
                .as_ref()
                .map_or(PathBuf::from(DEFAULT_PROVER_KEYS_PATH), PathBuf::from);

            self.zk_prover = Some(Some(ZKProverConfig { prover_keys_path }));
        }

        // Configure RPC server
        #[cfg(feature = "rpc-server")]
        {
            if let Some(rpc_config) = &config_file.rpc_server {
                let bind_to = match rpc_config.bind.as_ref() {
                    Some(ip_string) => match ip_string.parse::<IpAddr>() {
                        Ok(parsed) => Some(parsed),
                        Err(err) => {
                            return Err(Error::config_error(format!(
                                "Failed parsing RPC server address {err}"
                            )))
                        }
                    },
                    None => None,
                };

                let allow_ips = if rpc_config.allowip.is_empty() {
                    None
                } else {
                    let result = rpc_config
                        .allowip
                        .iter()
                        .map(|s| {
                            s.parse::<IpAddr>()
                                .map_err(|e| Error::config_error(format!("Invalid IP: {e}")))
                        })
                        .collect::<Result<Vec<IpAddr>, Error>>();
                    Some(result?)
                };

                let credentials = match (&rpc_config.username, &rpc_config.password) {
                    (Some(u), Some(p)) => Some(Credentials::new(u, p)),
                    (None, None) => None,
                    _ => {
                        return Err(Error::config_error(
                            "RTP: Either both username and password have to be set or none.",
                        ))
                    }
                };

                self.rpc_server = Some(Some(RpcServerConfig {
                    bind_to,
                    port: rpc_config.port.unwrap_or(consts::RPC_DEFAULT_PORT),
                    cors_domains: Some(rpc_config.cors_domains.clone()),
                    allow_ips,
                    allowed_methods: Some(rpc_config.methods.clone()),
                    credentials,
                }));
            }
        }

        // Configure metrics server
        #[cfg(feature = "metrics-server")]
        {
            if let Some(metrics_config) = &config_file.metrics_server {
                let ip = match metrics_config.bind.as_ref() {
                    Some(ip_string) => match ip_string.parse::<IpAddr>() {
                        Ok(parsed) => Some(parsed),
                        Err(err) => {
                            return Err(Error::config_error(format!(
                                "Failed parsing metrics server address {err}"
                            )))
                        }
                    },
                    None => None,
                };

                let addr = SocketAddr::new(
                    ip.unwrap_or_else(default_bind),
                    metrics_config.port.unwrap_or(consts::METRICS_DEFAULT_PORT),
                );

                let credentials =
                    match (&metrics_config.username, &metrics_config.password) {
                        (Some(u), Some(p)) => Some(Credentials::new(u, p)),
                        (None, None) => None,
                        _ => return Err(Error::config_error(
                            "Metrics: Either both username and password have to be set or none.",
                        )),
                    };

                self.metrics_server = Some(Some(MetricsServerConfig { addr, credentials }));
            }
        }

        #[cfg(feature = "nimiq-mempool")]
        {
            if let Some(mempool_settings) = &config_file.mempool {
                self.mempool = Some(MempoolConfig::from(mempool_settings.clone()));
            }
        }

        Ok(self)
    }

    /// Applies settings from the command line
    pub fn command_line(&mut self, command_line: &CommandLine) -> Result<&mut Self, Error> {
        // Set sync_mode
        if let Some(sync_mode) = command_line.sync_mode {
            self.consensus
                .get_or_insert_with(ConsensusConfig::default)
                .sync_mode = sync_mode.into()
        }

        // Set network ID
        if let Some(network_id) = command_line.network {
            self.network_id(network_id);
        }

        // NOTE: We're always return `Ok(_)`, but we might want to introduce errors later.
        Ok(self)
    }
}

/// Contains the configurations for the ZK proof generation.
#[derive(Debug, Clone, Builder)]
pub struct ZKProverConfig {
    /// Prover keys path for the zkp prover.
    pub prover_keys_path: PathBuf,
}
