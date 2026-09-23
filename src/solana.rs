//! Standard seed-based Ed25519 keys. The private seed is hashed and clamped by
//! ed25519-dalek; incrementing a scalar would NOT give a matching recoverable seed.
use crate::{address::AddressType, cpu, pattern::Pattern};
use anyhow::{anyhow, bail, Result};
use ed25519_dalek::SigningKey;
use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use zeroize::Zeroizing;

// No Debug or Display: seeds must never appear in diagnostics.
pub struct Candidate {
    pub seed: Zeroizing<[u8; 32]>,
    pub public: [u8; 32],
}
impl Candidate {
    pub fn from_seed(seed: Zeroizing<[u8; 32]>) -> Self {
        let key = SigningKey::from_bytes(&seed);
        Self {
            public: key.verifying_key().to_bytes(),
            seed,
        }
        // SigningKey implements ZeroizeOnDrop with the enabled zeroize feature.
    }
    fn random() -> Result<Self> {
        let mut seed = Zeroizing::new([0u8; 32]);
        getrandom::fill(&mut *seed)
            .map_err(|_| anyhow!("OS cryptographic random source failed"))?;
        Ok(Self::from_seed(seed))
    }
    pub fn address(&self) -> String {
        bs58::encode(self.public).into_string()
    }
    pub fn keypair_bytes(&self) -> Zeroizing<[u8; 64]> {
        let mut bytes = Zeroizing::new([0u8; 64]);
        bytes[..32].copy_from_slice(&*self.seed);
        bytes[32..].copy_from_slice(&self.public);
        bytes
    }
}
impl cpu::Worker for Candidate {
    type Found = Self;
    fn address(&self, _: AddressType) -> String {
        self.address()
    }
    fn advance(&mut self) -> Result<()> {
        *self = Self::random()?;
        Ok(())
    }
    fn into_candidate(self) -> Self {
        self
    }
}
pub fn run(
    pattern: &Pattern,
    threads: usize,
    benchmark: Option<Duration>,
    cancelled: Arc<AtomicBool>,
    progress: impl FnMut(u64, Duration),
) -> Result<cpu::Run<Candidate>> {
    if pattern.kind != AddressType::Solana {
        bail!("Solana worker requires a Solana pattern");
    }
    cpu::run_workers(
        pattern,
        threads,
        benchmark,
        cancelled,
        progress,
        Candidate::random,
    )
}
pub fn verify(candidate: &Candidate, pattern: &Pattern) -> Result<String> {
    let key = SigningKey::from_bytes(&candidate.seed);
    let address = candidate.address();
    if pattern.kind != AddressType::Solana
        || key.verifying_key().to_bytes() != candidate.public
        || !pattern.matches(&address)
    {
        bail!("Solana result verification failed; no private key will be printed");
    }
    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::Signer;
    #[test]
    fn search_cancellation_and_verification_fail_closed() {
        let p = Pattern::new("".into(), "".into(), false, AddressType::Solana).unwrap();
        let r = run(&p, 2, None, Arc::new(AtomicBool::new(true)), |_, _| {}).unwrap();
        assert!(r.found.is_none());
        assert_eq!(r.attempts, 0);
        let mut c = Candidate::random().unwrap();
        assert!(verify(&c, &p).is_ok());
        c.public[0] ^= 1;
        assert!(verify(&c, &p).is_err());
    }
    #[test]
    fn rfc8032_seed_public_key_and_signature() {
        // RFC8032 section 7.1, Ed25519 test 1, empty message.
        let hex = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
        let mut seed = Zeroizing::new([0u8; 32]);
        for (i, b) in seed.iter_mut().enumerate() {
            *b = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap();
        }
        let candidate = Candidate::from_seed(seed);
        assert_eq!(
            &*crate::address::hex(&candidate.public),
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
        );
        let key = SigningKey::from_bytes(&candidate.seed);
        let signature = key.sign(b"");
        assert_eq!(&*crate::address::hex(&signature.to_bytes()), "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b");
        assert_eq!(
            candidate.address(),
            "FVen3X669xLzsi6N2V91DoiyzHzg1uAgqiT8jZ9nS96Z"
        );
        assert!(SigningKey::from_keypair_bytes(&candidate.keypair_bytes()).is_ok());
    }
}
