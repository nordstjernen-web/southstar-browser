//! Southstar — the X25519MLKEM768 hybrid post-quantum key exchange for TLS 1.3 clients: RustCrypto's ML-KEM-768 joined with ring's X25519.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use ml_kem::{Ciphertext, Decapsulate, DecapsulationKey768, KeyExport, MlKem768, Seed};
use ring::rand::{SecureRandom, SystemRandom};
use rustls::crypto::{
    ActiveKeyExchange, CompletedKeyExchange, SharedSecret, SupportedKxGroup, ring::kx_group,
};
use rustls::ffdhe_groups::FfdheGroup;
use rustls::{Error, NamedGroup, PeerMisbehaved, ProtocolVersion};
use zeroize::Zeroize;

pub(crate) const SEED_LEN: usize = 64;
pub(crate) const CIPHERTEXT_LEN: usize = 1088;
pub(crate) const X25519_LEN: usize = 32;

const INVALID_KEY_SHARE: Error = Error::PeerMisbehaved(PeerMisbehaved::InvalidKeyShare);

#[derive(Debug)]
pub struct X25519MlKem768;

pub static X25519MLKEM768: &dyn SupportedKxGroup = &X25519MlKem768;

impl X25519MlKem768 {
    pub(crate) fn start_with_seed(
        &self,
        seed: [u8; SEED_LEN],
    ) -> Result<Box<dyn ActiveKeyExchange>, Error> {
        let key = DecapsulationKey768::from_seed(Seed::from(seed));
        let classical = kx_group::X25519.start()?;
        let mut share = key.encapsulation_key().to_bytes().to_vec();
        share.extend_from_slice(classical.pub_key());
        Ok(Box::new(Active {
            key,
            classical,
            share,
        }))
    }
}

impl SupportedKxGroup for X25519MlKem768 {
    fn start(&self) -> Result<Box<dyn ActiveKeyExchange>, Error> {
        let mut seed = [0u8; SEED_LEN];
        SystemRandom::new()
            .fill(&mut seed)
            .map_err(|_| Error::FailedToGetRandomBytes)?;
        let active = self.start_with_seed(seed);
        seed.zeroize();
        active
    }

    fn start_and_complete(&self, _client_share: &[u8]) -> Result<CompletedKeyExchange, Error> {
        Err(Error::General(String::from(
            "X25519MLKEM768 is implemented for TLS clients only",
        )))
    }

    fn ffdhe_group(&self) -> Option<FfdheGroup<'static>> {
        None
    }

    fn name(&self) -> NamedGroup {
        NamedGroup::X25519MLKEM768
    }

    fn usable_for_version(&self, version: ProtocolVersion) -> bool {
        version == ProtocolVersion::TLSv1_3
    }
}

struct Active {
    key: DecapsulationKey768,
    classical: Box<dyn ActiveKeyExchange>,
    share: Vec<u8>,
}

impl ActiveKeyExchange for Active {
    fn complete(self: Box<Self>, peer_pub_key: &[u8]) -> Result<SharedSecret, Error> {
        if peer_pub_key.len() != CIPHERTEXT_LEN + X25519_LEN {
            return Err(INVALID_KEY_SHARE);
        }
        let (ciphertext, classical_share) = peer_pub_key.split_at(CIPHERTEXT_LEN);
        let ciphertext =
            Ciphertext::<MlKem768>::try_from(ciphertext).map_err(|_| INVALID_KEY_SHARE)?;
        let mut post_quantum = self.key.decapsulate(&ciphertext);
        let classical = self.classical.complete(classical_share)?;
        let mut secret = post_quantum.to_vec();
        post_quantum.as_mut_slice().zeroize();
        secret.extend_from_slice(classical.secret_bytes());
        Ok(SharedSecret::from(secret))
    }

    fn hybrid_component(&self) -> Option<(NamedGroup, &[u8])> {
        Some((self.classical.group(), self.classical.pub_key()))
    }

    fn complete_hybrid_component(
        self: Box<Self>,
        peer_pub_key: &[u8],
    ) -> Result<SharedSecret, Error> {
        self.classical.complete(peer_pub_key)
    }

    fn pub_key(&self) -> &[u8] {
        &self.share
    }

    fn ffdhe_group(&self) -> Option<FfdheGroup<'static>> {
        None
    }

    fn group(&self) -> NamedGroup {
        NamedGroup::X25519MLKEM768
    }
}
