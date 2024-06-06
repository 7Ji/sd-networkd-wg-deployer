use std::{collections::{BTreeMap, HashMap}, fmt::Display, fs::{create_dir, create_dir_all, remove_dir_all, File}, io::{Read, Write}, path::{Path, PathBuf}};
use base64::Engine;
use serde::{de::DeserializeOwned, Deserialize};

const LEN_CURVE25519_KEY_RAW: usize = 32;
const LEN_CURVE25519_KEY_BASE64: usize = 44;

#[derive(Debug)]
enum Error {
    ArgumentNotRight,
    Base64EncodeBufferTooSmall,
    Base64LengthIncorrect {
        expected: usize, actual: usize
    },
    Base64DecodeError (String),
    Base64DecodeBufferTooSmall,
    ImpossibleLogic,
    IoError (String),
    YAMLError (String),
}

impl From<base64::EncodeSliceError> for Error {
    fn from(_: base64::EncodeSliceError) -> Self {
        Self::Base64EncodeBufferTooSmall
    }
}

impl From<base64::DecodeSliceError> for Error {
    fn from(value: base64::DecodeSliceError) -> Self {
        match value {
            base64::DecodeSliceError::DecodeError(e) => e.into(),
            base64::DecodeSliceError::OutputSliceTooSmall => 
                Self::Base64DecodeBufferTooSmall,
        }
    }
}

#[inline(always)]
fn string_from_display<D: Display>(display: D) -> String {
    format!("{}", display)
}

macro_rules! impl_from_error_display {
    ($external: ty, $internal: ident) => {
        impl From<$external> for Error {
            fn from(value: $external) -> Self {
                Self::$internal(string_from_display(value))
            }
        }
    };
}

impl_from_error_display!(std::io::Error, IoError);
impl_from_error_display!(serde_yaml::Error, YAMLError);
impl_from_error_display!(base64::DecodeError, Base64DecodeError);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::ArgumentNotRight => write!(f, "Argument not right"),
            Error::Base64EncodeBufferTooSmall => 
                write!(f, "Base64 encode buffer too small"),
            Error::Base64LengthIncorrect { expected, actual } => 
                write!(f, "Base64 length incorrect, expected {}, actual {}",
                    expected, actual),
            Error::Base64DecodeError(e) => 
                write!(f, "Base64 decode error: {}", e),
            Error::Base64DecodeBufferTooSmall => 
                write!(f, "Base64 decode buffer too small"),
            Error::ImpossibleLogic => write!(f, "Impossible logic"),
            Error::IoError(e) => write!(f, "IO Error: {}", e),
            Error::YAMLError(e) => write!(f, "YAML Error: {}", e),
        }
    }
}

type Result<T> = std::result::Result<T, Error>;

fn file_create_checked<P: AsRef<Path>>(path: P) -> Result<File> {
    File::create(&path).map_err(|e|{
        eprintln!("Failed to create file at '{}': {}", 
                    path.as_ref().display(), e);
        e.into()
    })
}

fn write_all_checked<W: Write>(writer: &mut W, data: &[u8]) -> Result<()> {
    writer.write_all(data).map_err(|e|{
        eprintln!("Failed to write {} bytes to file: {}", data.len(), e);
        e.into()
    })
}

fn file_open_checked<P: AsRef<Path>>(path: P) -> Result<File> {
    File::open(&path).map_err(|e|{
        eprintln!("Failed to open file at '{}': {}", 
                    path.as_ref().display(), e);
        e.into()
    })
}

fn read_exact_checked<R: Read>(reader: &mut R, data: &mut [u8]) -> Result<()> {
    reader.read_exact(data).map_err(|e|{
        eprintln!("Failed to read {} bytes from file: {}", data.len(), e);
        e.into()
    })
}

fn create_dir_all_checked<P: AsRef<Path>>(path: P) -> Result<()> {
    create_dir_all(&path).map_err(|e|{
        eprintln!("Failed to create dir '{}': {}", path.as_ref().display(), e);
        e.into()
    })
}

fn content_to_file<P: AsRef<Path>>(content: &[u8], path: P) -> Result<()> {
    write_all_checked(&mut file_create_checked(path)?, content)
}

fn yaml_from_reader_checked<T, R>(reader: &mut R) -> Result<T> 
where
    T: DeserializeOwned,
    R: Read
{
    serde_yaml::from_reader(reader).map_err(Into::into)
}

/// A raw WireGuard key, users shall not use this, but `WireGuardKey` instead
type WireGuardKeyRaw = [u8; WireGuardKey::LEN_RAW];
/// A base64-encoded WireGuard key
type WireGuardKeyBase64 = [u8; WireGuardKey::LEN_BASE64];

/// A WireGuard-compatible key, does not differentiate public or private by 
/// itself, user should take care of that
#[derive(Debug, Default)]
struct WireGuardKey {
    value: WireGuardKeyRaw
}


impl WireGuardKey {
    /// The length of a WireGuard key, raw byte length
    const LEN_RAW: usize = LEN_CURVE25519_KEY_RAW;
    /// The length of a WireGuard key, base64 encoded length
    const LEN_BASE64: usize = LEN_CURVE25519_KEY_BASE64;

    /// The base64 engine we use, chars `0-9` `a-z` `A-Z` `/` `+`, with padding
    const BASE64_ENGINE: base64::engine::GeneralPurpose 
        = base64::engine::general_purpose::STANDARD;

    fn new_empty_raw() -> WireGuardKeyRaw {
        [0; Self::LEN_RAW]
    }

    fn new_empty_base64() -> WireGuardKeyBase64 {
        [0; Self::LEN_BASE64]
    }

    /// Create a new random `WireGuardKey` with a `rand::Rng`-compatible 
    /// generator
    fn new_with_generator<G: rand::Rng>(mut generator: G) -> Self {
        let mut value = Self::new_empty_raw();
        generator.fill_bytes(&mut value);
        Self { value }
    }

    /// Create a new random `WireGuardKey`, with a `rand::thread_rng()` random
    /// generator
    fn new() -> Self {
        Self::new_with_generator(rand::thread_rng())
    }

    /// Encode this key to base64, note it is still raw bytes, users want a 
    /// `String` shall call `base64_string()` instead
    fn base64(&self) -> Result<WireGuardKeyBase64> {
        let mut buffer = Self::new_empty_base64();
        let size = Self::BASE64_ENGINE
            .encode_slice(&self.value, &mut buffer)?;
        if size == Self::LEN_BASE64 {
            Ok(buffer)
        } else {
            Err(Error::Base64LengthIncorrect {
                expected: Self::LEN_BASE64,
                actual: size,
            })
        }
    }

    /// Encode this key to base64 string
    fn base64_string(&self) -> String {
        Self::BASE64_ENGINE.encode(self.value)
    }

    /// Get the corresponding public key, assuming this is a private key.
    /// 
    /// As we don't differentiate on public key or private key, it's totally
    /// legal to generate a public key of a public key, but that would be of
    /// no use
    fn pubkey(&self) -> Self {
        let value = curve25519_dalek::EdwardsPoint::mul_base_clamped(
            self.value).to_montgomery().to_bytes();
        Self { value }
    }

    /// Write this key to file, without encoding
    fn to_file_raw<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        write_all_checked(
            &mut file_create_checked(path)?, &self.value)
    }

    /// Write this key to file, base64 encoded
    fn to_file_base64<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let base64 = self.base64()?;
        write_all_checked(&mut file_create_checked(path)?, &base64)
    }

    /// Read from file, in which a key is stored base64-encoded
    fn from_file_base64<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut base64 = Self::new_empty_base64();
        read_exact_checked(
            &mut file_open_checked(path)?, &mut base64)?;
        let mut value = Self::new_empty_raw();
        Self::BASE64_ENGINE.decode_slice(&base64, &mut value)?;
        Ok( Self { value } )
    }

    /// Read from file, in which a key is stored as raw un-encoded bytes
    fn from_file_raw<P: AsRef<Path>>(path: P) -> Result<Self> {
        let mut value = Self::new_empty_raw();
        read_exact_checked(
            &mut file_open_checked(path)?, &mut value)?;
        Ok( Self { value } )
    }

    /// Read from file if it exists, otherwise generate a new one
    fn from_file_raw_or_new<P: AsRef<Path>>(path: P) -> Result<Self> {
        if path.as_ref().exists() {
            return Self::from_file_raw(path)
        }
        let key = Self::new();
        key.to_file_raw(path)?;
        Ok(key)
    }

    /// Read from file if it exists, otherwise generate a new one
    fn from_file_base64_or_new<P: AsRef<Path>>(path: P) -> Result<Self> {
        if path.as_ref().exists() {
            return Self::from_file_base64(path)
        }
        let key = Self::new();
        key.to_file_base64(path)?;
        Ok(key)
    }
}

type PeerList = BTreeMap<String, PeerConfig>;

#[derive(Debug, Deserialize)]
/// Config of a peer
struct PeerConfig {
    /// The IP of the peer inside this network
    ip: String,
    /// The .netdev unit name, without `.netdev` suffix, (the suffix would be 
    /// appended automatically), e.g. `30-wireguard`, if this is empty
    /// then the global `netdev` would be used.
    #[serde(default)]
    netdev: String,
    /// The .network unit name, without `.network` suffix, (the suffix would be 
    /// appended automatically), e.g. `40-wireguard`, if this is empty
    /// then the global `network` would be used
    #[serde(default)]
    network: String,
    /// The interface name, if this is kept empty then the global `iface` would
    /// be used
    #[serde(default)]
    iface: String,
    /// The endpoint, i.e. the IP outside this network that other peers can 
    /// connect accordingly, usually a host + port pair
    #[serde(default)]
    endpoint: String,
    /// IP ranges outside of the main wireguard range that should be forwarded
    /// into the wireguard range
    #[serde(default)]
    forward: Vec<String>,
    /// Peer names this peer is able to connect directly in the same level
    /// - As a child, a peer is always able to connect to its parent
    /// - If not set (as `None`), this peer is able to connect to any other peer 
    /// directly at the same level
    /// - If set (as `Some`), this peer is only able to connect listed peers at
    /// the same level directly, even if it's empty, in that case it would only
    /// be able to connect to its parent directly
    direct: Option<Vec<String>>,
    /// Child peers connected under this peer.
    /// - Peers living as child can always connect to their parent. If none of 
    /// children can connect to other peers, this is essentially a star network
    /// - Peers, if not explicitly disallowed, can connect to any other peer
    /// in the same level, this is essentially a full mesh network
    #[serde(default)]
    children: PeerList
}

// fn peer_reachable(peer_name: &String, peer_config: &PeerConfig, endpoint_name: &String, endpoint_config: &PeerConfig) -> bool {
//     match (&peer_config.reach, &endpoint_config.reach) {
//         (Some(peer_reach), Some(endpoint_reach)) => peer_reach.contains(endpoint_name) && endpoint_reach.contains(peer_name),
//         (Some(peer_reach), None) => peer_reach.contains(endpoint_name),
//         (None, Some(endpoint_reach)) => endpoint_reach.contains(peer_name),
//         (None, None) => true,
//     }
// }

#[derive(Debug, Deserialize)]
struct Config {
    /// Whether to generate pre-shared key for each peer pair
    #[serde(default)]
    psk: bool,
    /// The .netdev unit name, without `.netdev` suffix, (the suffix would be 
    /// appended automatically), e.g. `30-wireguard`
    netdev: String,
    /// The .network unit name, without `.network` suffix, (the suffix would be 
    /// appended automatically), e.g. `40-wireguard`
    network: String,
    /// The interface name, e.g. `wg0`
    iface: String,
    /// The list of peers
    peers: PeerList,
}

impl Config {
    fn finalize(&mut self) {
        if ! self.netdev.ends_with(".netdev") {
            self.netdev.push_str(".netdev")
        }
        if ! self.network.ends_with(".network") {
            self.network.push_str(".network")
        }
    }
}


/// A wireguard key in netdev that shall be stored in a file
#[derive(Debug, Default)]
struct NetDevKeyFile {
    /// The backing key
    key: WireGuardKey,
    /// The file name this key shall be stored, inside the folder
    /// `/etc/systemd/network/keys/wg`
    filename: String,
}

/// A wireguard peer in a netdev
#[derive(Debug, Default)]
struct NetDevPeer {
    /// The incoming IP ranges this is allowed to access, also hinting whether
    /// traffic should go through this peer if a corresponding range is found
    allowed: Vec<String>,
    /// The public key of this peer
    pubkey: WireGuardKey,
    /// The endpoint
    endpoint: String,
    /// The pre-shared key between the peer
    psk: Option<NetDevKeyFile>,
}

/// A .netdev config
#[derive(Debug, Default)]
struct NetDevConfig {
    name: String,
    /// The private key of the netdev
    key: NetDevKeyFile,
    /// The peers
    peers: Vec<NetDevPeer>
}

/// A .network config
#[derive(Debug, Default)]
struct NetWorkConfig {
    name: String,
    address: String,
    routes: Vec<String>,
}

/// A .netdev + .network config
#[derive(Debug, Default)]
struct CompositeConfig {
    iface: String,
    netdev: NetDevConfig,
    network: NetWorkConfig
}


#[derive(Debug, Default)]
struct ConfigsToWrite {
    map: BTreeMap<String, CompositeConfig>
}

impl ConfigsToWrite {
    fn try_add_peer(&mut self, dir_keys: &Path,
        config: &Config, peer_name: &str, peer_config: &PeerConfig
    ) -> Result<()> 
    {
        macro_rules! string_non_empty_or_global {
            ($name: ident) => {
                if peer_config.$name.is_empty() {
                    config.$name.clone()
                } else {
                    peer_config.$name.clone()
                }
            };
        }
        let composite = CompositeConfig {
            iface: string_non_empty_or_global!(iface),
            netdev: NetDevConfig {
                name: string_non_empty_or_global!(netdev),
                key: {
                    let filename = format!("private-{}", peer_name);
                    let key = 
                        WireGuardKey::from_file_base64_or_new(
                            &dir_keys.join(&filename))?;
                    NetDevKeyFile { key, filename }
                },
                peers: Default::default(),
            },
            network: NetWorkConfig {
                name: string_non_empty_or_global!(network),
                address: peer_config.ip.clone(),
                routes: Default::default(),
            },
        };
        match self.map.insert(peer_name.to_string(), composite) {
            Some(_) => {
                eprintln!("Duplicated peer {}, impossible", peer_name);
                Err(Error::ImpossibleLogic)
            },
            None => Ok(()),
        }
    }

    fn try_add_peers(&mut self, 
        dir_keys: &Path, config: &Config, peers: &PeerList
    ) -> Result<()> 
    {
        for (peer_name, peer_config) in peers.iter() {
            self.try_add_peer(dir_keys, config, peer_name, peer_config)?;
            self.try_add_peers(dir_keys, config, &peer_config.children)?;
        }
        Ok(())
    }

    fn try_from_config<P: AsRef<Path>>(config: &Config, dir_all: P) -> Result<Self> {
        let mut result = Self::default();
        let dir_keys = dir_all.as_ref().join("keys");
        create_dir_all_checked(&dir_keys)?;
        result.try_add_peers(&dir_keys, config, &config.peers)?;
        
        // let mut preshared_keys =HashMap::new();
        // if config.psk { 
        //     let mut names: Vec<&String> = config.peers.keys().collect();
        //     names.sort_unstable();
        //     let mut name_key = "psk-".to_string();
        //     let len_prefix = name_key.len();
        //     for i in 0..config.peers.len() {
        //         let some = names[i];
        //         name_key.truncate(len_prefix);
        //         name_key.push_str(some);
        //         name_key.push('-');
        //         let len_prefix = name_key.len();
        //         for j in i+1..config.peers.len() {
        //             let other = names[j];
        //             name_key.truncate(len_prefix);
        //             name_key.push_str(other);
        //             let path_key = dir_keys.join(&name_key);
        //             let key = 
        //                 WireGuardKey::from_file_base64_or_new(&path_key)?;
        //             if preshared_keys.insert((some, other), key).is_some(){
        //                 eprintln!("Duplicated preshared-key pair for {} and {},
        //                     impossible", some, other);
        //                 return Err(Error::ImpossibleLogic)
        //             }
        //         }
        //     }
        // }
        // let mut keys = HashMap::new();
        // {
        //     let mut name_key = "private-".to_string();
        //     let len_prefix = name_key.len();
        //     for name in config.peers.keys() {
        //         name_key.truncate(len_prefix);
        //         name_key.push_str(name);
        //         let path_key = dir_keys.join(&name_key);
        //         let key = 
        //             WireGuardKey::from_file_base64_or_new(&path_key)?;
        //         let pubkey = key.pubkey();
        //         if keys.insert(name, (key, pubkey)).is_some() {
        //             eprintln!("Duplicated private & public key for {},
        //                 impossible", name);
        //             return Err(Error::ImpossibleLogic)
        //         }
        //     }
        // }
        // self.tr
        Ok(result)
    }

    fn try_write<P: AsRef<Path>>(&self, dir_all: P) -> Result<()> {
        let dir_configs = dir_all.as_ref().join("configs");
        let _ = remove_dir_all(&dir_configs);
        create_dir_all_checked(&dir_configs)?;
        // create_dir_all_checked(dir_keys)?;
        let mut buffer = String::new();
        for (name, config) in self.map.iter() {
            let dir_config = dir_configs.join(name);
            let dir_keys = dir_config.join("keys/wg");
            create_dir_all_checked(&dir_keys)?;

            let netdev = &config.netdev;
            buffer.clear();
            buffer.push_str("[NetDev]\nName=");
            macro_rules! buffer_add_key_file {
                ($key_file: expr) => {
                    buffer.push_str(&$key_file.filename);
                    content_to_file(&$key_file.key.base64()?, 
                        &dir_keys.join(&$key_file.filename))?;
                };
            }
            buffer.push_str(&config.iface);
            buffer.push_str("\n\
                Kind=wireguard\n\n\
                [WireGuard]\n\
                ListenPort=51820\n\
                PrivateKeyFile=/etc/systemd/network/keys/wg/");
            buffer_add_key_file!(netdev.key);
            buffer.push('\n');
            for peer in netdev.peers.iter() {
                buffer.push_str("\n[WireGuardPeer]\nPublicKey=");
                buffer.push_str(&peer.pubkey.base64_string());
                if let Some(psk) = &peer.psk {
                    buffer.push_str("\nPreSharedKeyFile=");
                    buffer_add_key_file!(psk);
                }
                if ! peer.endpoint.is_empty() {
                    buffer.push_str("\nEndpoint=");
                    buffer.push_str(&peer.endpoint);
                }
                for allowed in peer.allowed.iter() {
                    buffer.push_str("\nAllowedIPs=");
                    buffer.push_str(&allowed);
                }
                buffer.push('\n');
            }
            content_to_file(buffer.as_bytes(), 
                &dir_config.join(
                    format!("{}.netdev", &netdev.name)))?;
            
            let network = &config.network;
            buffer.clear();
            buffer.push_str("[Match]\nName=");
            buffer.push_str(&config.iface);
            buffer.push_str("\n\n[Network]\nAddress=");
            buffer.push_str(&network.address);
            for route in network.routes.iter() {
                buffer.push_str("\n\n[Route]\nDestination=");
                buffer.push_str(route);
                buffer.push_str("\nScope=link");
            }
            buffer.push('\n');
            content_to_file(buffer.as_bytes(),
                &dir_config.join(
                    format!("{}.network", network.name)))?;
        }
        Ok(())
    }
}


fn main() -> Result<()> { // arg1: config file, arg2: output dir
    let mut args = std::env::args_os();
    let config = args.nth(1).ok_or(Error::ArgumentNotRight)?;
    let output = args.next().ok_or(Error::ArgumentNotRight)?;
    let mut file = file_open_checked(&config)?;
    let mut config: Config = yaml_from_reader_checked(&mut file)?;
    config.finalize();
    let configs_to_write = 
        ConfigsToWrite::try_from_config(&config, &output)?;
    configs_to_write.try_write(&output)
}
