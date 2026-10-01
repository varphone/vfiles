use crate::options::{FtpsClientAuth, TlsFlags};
use rustls::{
    NoKeyLog, RootCertStore, ServerConfig, SupportedProtocolVersion,
    pki_types::{
        CertificateDer, PrivateKeyDer,
        pem::{self, PemObject},
    },
    server::{
        ClientCertVerifierBuilder, NoServerSessionStorage, ProducesTickets, StoresServerSessions,
        WebPkiClientVerifier,
    },
    version::{TLS12, TLS13},
};

// Enable aws_lc_rs, unless the flag is disabled (in which case ring has to be enabled).
// If both are enabled, aws_lc_rs is preferred.
#[cfg(feature = "aws_lc_rs")]
use rustls::crypto::{aws_lc_rs as crypto_impl, aws_lc_rs::Ticketer};
#[cfg(all(not(feature = "aws_lc_rs"), feature = "ring"))]
use rustls::crypto::{ring as crypto_impl, ring::Ticketer};

use std::{
    fmt::{self, Formatter},
    fs::File,
    io::{self, BufReader},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use thiserror::Error;

// FTPSConfig shows how TLS security is configured for the server or a particular channel.
#[derive(Clone)]
pub enum FtpsConfig {
    Off,
    Building { certs_file: PathBuf, key_file: PathBuf },
    On {
        tls_config: Arc<ServerConfig>,
        data_tls_config: Option<Arc<ServerConfig>>,
        data_resumption: Option<Arc<AtomicBool>>,
    },
}

impl fmt::Debug for FtpsConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            FtpsConfig::Off => write!(f, "Off"),
            FtpsConfig::Building { .. } => write!(f, "Building"),
            FtpsConfig::On { .. } => write!(f, "On"),
        }
    }
}

impl FtpsConfig {
    /// Give each FTP control session its own TLS resumption state. The data-channel view is
    /// read-only and only accepts tickets or session IDs issued by the control handshake.
    pub(crate) fn for_control_session(&self) -> Result<Self, rustls::Error> {
        let Self::On { tls_config, .. } = self else {
            return Ok(self.clone());
        };

        let mut session_config = (**tls_config).clone();
        session_config.session_storage = if tls_config.session_storage.can_cache() {
            TlsSessionCache::new(1024)
        } else {
            Arc::new(NoServerSessionStorage {})
        };
        if tls_config.ticketer.enabled() {
            session_config.ticketer = Ticketer::new()?;
        }

        let control_tls_config = Arc::new(session_config);
        let data_resumption = Arc::new(AtomicBool::new(false));
        let mut data_config = (*control_tls_config).clone();
        data_config.session_storage = Arc::new(ReadOnlySessionStore {
            inner: Arc::clone(&control_tls_config.session_storage),
            resumed_control_session: Arc::clone(&data_resumption),
        });
        data_config.ticketer = Arc::new(ControlSessionTicketGate {
            inner: Arc::clone(&control_tls_config.ticketer),
            resumed_control_session: Arc::clone(&data_resumption),
        });
        // TLS 1.3 does not need a replacement ticket for a data connection. Keeping the
        // control ticket lets clients reuse it for each transfer.
        data_config.send_tls13_tickets = 0;

        Ok(Self::On {
            tls_config: control_tls_config,
            data_tls_config: Some(Arc::new(data_config)),
            data_resumption: Some(data_resumption),
        })
    }
}

#[derive(Debug)]
struct ReadOnlySessionStore {
    inner: Arc<dyn StoresServerSessions>,
    resumed_control_session: Arc<AtomicBool>,
}

impl StoresServerSessions for ReadOnlySessionStore {
    fn put(&self, _key: Vec<u8>, _value: Vec<u8>) -> bool {
        // A full data-channel handshake must not create resumption state that can later pass
        // the data-channel session-reuse check.
        false
    }

    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        let value = self.inner.get(key);
        if value.is_some() {
            self.resumed_control_session.store(true, Ordering::SeqCst);
        }
        value
    }

    fn take(&self, key: &[u8]) -> Option<Vec<u8>> {
        let value = self.inner.take(key);
        if value.is_some() {
            self.resumed_control_session.store(true, Ordering::SeqCst);
        }
        value
    }

    fn can_cache(&self) -> bool {
        self.inner.can_cache()
    }
}

#[derive(Debug)]
struct ControlSessionTicketGate {
    inner: Arc<dyn ProducesTickets>,
    resumed_control_session: Arc<AtomicBool>,
}

impl ProducesTickets for ControlSessionTicketGate {
    fn enabled(&self) -> bool {
        self.inner.enabled()
    }

    fn lifetime(&self) -> u32 {
        self.inner.lifetime()
    }

    fn encrypt(&self, plaintext: &[u8]) -> Option<Vec<u8>> {
        self.resumed_control_session
            .load(Ordering::SeqCst)
            .then(|| self.inner.encrypt(plaintext))
            .flatten()
    }

    fn decrypt(&self, ticket: &[u8]) -> Option<Vec<u8>> {
        let plaintext = self.inner.decrypt(ticket);
        if plaintext.is_some() {
            self.resumed_control_session.store(true, Ordering::SeqCst);
        }
        plaintext
    }
}

#[allow(dead_code)]
#[derive(Debug, Copy, Clone)]
pub struct FtpsNotAvailable;

impl fmt::Display for FtpsNotAvailable {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "FTPS not configured/available")
    }
}

impl std::error::Error for FtpsNotAvailable {}

// The error returned by new_config
#[derive(Error, Debug)]
#[error("TLS configuration error")]
pub enum ConfigError {
    #[error("found no private key")]
    NoPrivateKey,

    #[error("error reading key/cert input")]
    Load(#[from] io::Error),

    #[error("error reading PEM file")]
    LoadPem(#[from] pem::Error),

    #[error("error building root certs")]
    RootCerts(rustls::Error),

    #[error("error initialising Rustls")]
    RustlsInit(#[from] rustls::Error),

    #[error("error initialising the client cert verifier")]
    ClientVerifier(#[from] rustls::server::VerifierBuilderError),
}

pub fn new_config<P: AsRef<Path>>(
    certs_file: P,
    key_file: P,
    flags: TlsFlags,
    client_auth: FtpsClientAuth,
    trust_store: P,
) -> Result<Arc<ServerConfig>, ConfigError> {
    let certs: Vec<CertificateDer> = load_certs(certs_file)?;
    let privkey: PrivateKeyDer = load_private_key(key_file)?;

    let client_auther = match client_auth {
        FtpsClientAuth::Off => Ok(WebPkiClientVerifier::no_client_auth()),
        FtpsClientAuth::Request => {
            let builder: ClientCertVerifierBuilder = WebPkiClientVerifier::builder(Arc::new(root_cert_store(trust_store)?));
            builder.allow_unauthenticated().build()
        }
        FtpsClientAuth::Require => {
            let builder: ClientCertVerifierBuilder = WebPkiClientVerifier::builder(Arc::new(root_cert_store(trust_store)?));
            builder.build()
        }
    }
    .map_err(ConfigError::ClientVerifier)?;

    let mut versions: Vec<&SupportedProtocolVersion> = vec![];
    if flags.contains(TlsFlags::V1_2) {
        versions.push(&TLS12)
    }
    if flags.contains(TlsFlags::V1_3) {
        versions.push(&TLS13)
    }

    let provider = Arc::new(crypto_impl::default_provider());
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&versions)
        .map_err(ConfigError::RustlsInit)?
        .with_client_cert_verifier(client_auther)
        .with_single_cert(certs, privkey)
        .map_err(ConfigError::RustlsInit)?; // No SNI, single certificate

    // Support session resumption with server side state (Session IDs)
    config.session_storage = if flags.contains(TlsFlags::RESUMPTION_SESS_ID) {
        TlsSessionCache::new(1024)
    } else {
        Arc::new(NoServerSessionStorage {})
    };
    // Support session resumption with tickets. See https://tools.ietf.org/html/rfc5077
    if flags.contains(TlsFlags::RESUMPTION_TICKETS) {
        config.ticketer = Ticketer::new().map_err(ConfigError::RustlsInit)?;
    };
    // Don't allow dumping session keys
    config.key_log = Arc::new(NoKeyLog {});

    Ok(Arc::new(config))
}

fn root_cert_store<P: AsRef<Path>>(trust_pem: P) -> Result<RootCertStore, ConfigError> {
    let mut store = RootCertStore::empty();
    let certs = load_certs(trust_pem)?;
    for cert in certs.iter() {
        store.add(cert.clone()).map_err(ConfigError::RootCerts)?
    }
    Ok(store)
}

fn load_certs<P: AsRef<Path>>(filename: P) -> Result<Vec<CertificateDer<'static>>, ConfigError> {
    let certfile: File = File::open(filename)?;
    let mut reader: BufReader<File> = BufReader::new(certfile);
    Ok(CertificateDer::pem_reader_iter(&mut reader).collect::<Result<_, pem::Error>>()?)
}

fn load_private_key<P: AsRef<Path>>(filename: P) -> Result<PrivateKeyDer<'static>, ConfigError> {
    let keyfile = File::open(&filename)?;
    let mut reader = BufReader::new(keyfile);

    if let Some(key) = PrivateKeyDer::pem_reader_iter(&mut reader).next() {
        return Ok(key?);
    }
    Err(ConfigError::NoPrivateKey)
}

/// Stores the session IDs server side.
#[derive(Debug)]
struct TlsSessionCache {
    cache: moka::sync::Cache<Vec<u8>, Vec<u8>>,
}

impl TlsSessionCache {
    /// Make a new TlsSessionCache.  `size` is the maximum
    /// number of stored sessions.
    pub fn new(size: u64) -> Arc<TlsSessionCache> {
        debug_assert!(size > 0);
        Arc::new(TlsSessionCache {
            cache: moka::sync::CacheBuilder::new(size).time_to_idle(Duration::from_secs(5 * 60)).build(),
        })
    }
}

impl StoresServerSessions for TlsSessionCache {
    fn put(&self, key: Vec<u8>, value: Vec<u8>) -> bool {
        self.cache.insert(key, value);
        true
    }

    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.cache.get(&key.to_vec())
    }

    fn take(&self, key: &[u8]) -> Option<Vec<u8>> {
        let key_as_vec = key.to_vec();
        self.cache.get(&key_as_vec)
        // For some reason rustls always calls take and so removes the session ID which then breaks
        // FileZilla for instance. So I implement take here to not really take, only get...
        // self.cache.invalidate(&key_as_vec);
    }

    fn can_cache(&self) -> bool {
        true
    }
}
