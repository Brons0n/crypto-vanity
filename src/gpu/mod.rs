use crate::{
    cpu::{self, Candidate, Run},
    pattern::Pattern,
};
use anyhow::{bail, Result};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[cfg(all(feature = "gpu-metal", target_os = "macos"))]
pub mod metal;
#[cfg(feature = "gpu-opencl")]
pub mod opencl;

pub const LANES: usize = 256;
pub const STEPS: u64 = 16;
pub const PARAM_WORDS: usize = 84;

/// A backend owns persistent public points. init uploads independent public seeds;
/// dispatch_batch advances each lane by STEPS, checking both patterns on-device;
/// retrieve_match reads only a status word on a miss and a lane/offset on a hit.
pub trait GpuBackend {
    fn name(&self) -> &str;
    fn init(&mut self, points: &[u32], pattern: &Pattern) -> Result<()>;
    fn dispatch_batch(&mut self) -> Result<u64>;
    fn retrieve_match(&mut self) -> Result<Option<(usize, u32)>>;
}

pub fn select() -> Result<Box<dyn GpuBackend>> {
    #[cfg(all(feature = "gpu-metal", target_os = "macos"))]
    {
        Ok(Box::new(metal::Metal::new()?))
    }
    #[cfg(all(
        feature = "gpu-opencl",
        not(all(feature = "gpu-metal", target_os = "macos"))
    ))]
    {
        Ok(Box::new(opencl::OpenCl::new()?))
    }
    #[cfg(not(any(feature = "gpu-opencl", all(feature = "gpu-metal", target_os = "macos"))))]
    bail!("no GPU backend compiled for this OS; use gpu-metal on macOS or gpu-opencl on Windows/Linux")
}

pub fn parameters(p: &Pattern) -> Result<Vec<u32>> {
    if !p.kind.is_bitcoin() {
        bail!("GPU backends currently support Bitcoin only");
    }
    // Public API callers can construct Pattern directly; revalidate before FFI.
    Pattern::new(p.prefix.clone(), p.suffix.clone(), p.insensitive, p.kind)?;
    let mut v = vec![
        u32::from(p.kind == crate::address::AddressType::Bech32),
        u32::from(p.insensitive),
        p.prefix.len() as u32,
        p.suffix.len() as u32,
    ];
    v.extend(p.prefix.bytes().chain(p.suffix.bytes()).map(u32::from));
    v.resize(PARAM_WORDS, 0);
    Ok(v)
}

pub fn point_words(public: &secp256k1::PublicKey) -> Vec<u32> {
    let bytes = public.serialize_uncompressed();
    let mut words = Vec::with_capacity(24);
    for coord in [&bytes[1..33], &bytes[33..65]] {
        for chunk in coord.rchunks_exact(4) {
            words.push(u32::from_be_bytes(chunk.try_into().unwrap()));
        }
    }
    words.push(1);
    words.resize(24, 0);
    words
}

pub fn run(
    backend: &mut dyn GpuBackend,
    p: &Pattern,
    benchmark: Option<Duration>,
    cancelled: Arc<AtomicBool>,
    mut progress: impl FnMut(u64, Duration),
) -> Result<Run> {
    if !p.kind.is_bitcoin() {
        bail!("GPU backends currently support Bitcoin only");
    }
    let secp = cpu::context()?;
    let mut bases = Vec::with_capacity(LANES);
    let mut points = Vec::with_capacity(LANES * 24);
    // Leave >2^64 scalars before the group order so a lane cannot reach infinity.
    let mut limit = cpu::ORDER;
    limit[23] -= 1;
    limit[24..].fill(0);
    for _ in 0..LANES {
        let base = loop {
            let c = Candidate::random(&secp)?;
            if *c.secret < limit {
                break c;
            }
        };
        points.extend(point_words(&base.public));
        bases.push(base);
    }
    backend.init(&points, p)?;
    let start = Instant::now();
    let mut last = Instant::now();
    let mut offset = 0u64;
    let mut attempts = 0u64;
    while !cancelled.load(Ordering::Relaxed) {
        let count = backend.dispatch_batch()?;
        if count != LANES as u64 * STEPS {
            bail!("GPU reported an invalid batch size");
        }
        attempts = attempts
            .checked_add(count)
            .ok_or_else(|| anyhow::anyhow!("GPU attempt counter exhausted"))?;
        if let Some((lane, step)) = backend.retrieve_match()? {
            if lane >= LANES || step as u64 >= STEPS {
                bail!("GPU reported an invalid match offset");
            }
            if benchmark.is_none() {
                let delta = offset
                    .checked_add(step as u64)
                    .ok_or_else(|| anyhow::anyhow!("GPU offset exhausted"))?;
                let mut scalar = zeroize::Zeroizing::new([0u8; 32]);
                scalar[24..].copy_from_slice(&delta.to_be_bytes());
                let mut tweak = secp256k1::Scalar::from_be_bytes(*scalar)
                    .map_err(|_| anyhow::anyhow!("invalid GPU offset"))?;
                let mut key = secp256k1::SecretKey::from_byte_array(*bases[lane].secret)?;
                let derived = key.add_tweak(&tweak);
                key.non_secure_erase();
                tweak.non_secure_erase();
                let mut derived = derived?;
                let candidate = Candidate {
                    secret: zeroize::Zeroizing::new(derived.secret_bytes()),
                    public: secp256k1::PublicKey::from_secret_key(&secp, &derived),
                };
                derived.non_secure_erase();
                cpu::verify(&candidate, p)?;
                return Ok(Run {
                    found: Some(candidate),
                    attempts,
                    elapsed: start.elapsed(),
                });
            }
        }
        offset = offset
            .checked_add(STEPS)
            .ok_or_else(|| anyhow::anyhow!("GPU offset exhausted"))?;
        if benchmark.is_some_and(|d| start.elapsed() >= d) {
            break;
        }
        if last.elapsed() >= Duration::from_millis(250) {
            progress(attempts, start.elapsed());
            last = Instant::now();
        }
    }
    Ok(Run {
        found: None,
        attempts,
        elapsed: start.elapsed(),
    })
}

/// Mandatory startup test: full exact addresses require correct EC, both hashes,
/// both encodings, prefix/suffix checks, and persistent point advancement.
pub fn self_test(backend: &mut dyn GpuBackend) -> Result<()> {
    use crate::address::{self, AddressType};
    let secp = cpu::context()?;
    let mut bytes = [0u8; 32];
    bytes[31] = 1;
    let mut key = secp256k1::SecretKey::from_byte_array(bytes)?;
    let base = secp256k1::PublicKey::from_secret_key(&secp, &key);
    key.non_secure_erase();
    let mut points = Vec::new();
    for _ in 0..LANES {
        points.extend(point_words(&base));
    }
    for kind in [AddressType::Legacy, AddressType::Bech32] {
        let mut target = base;
        for _ in 0..STEPS + 7 {
            target = target.combine(&cpu::generator())?;
        }
        let address = address::derive(&target, kind);
        let body = &address[kind.fixed().len()..];
        let p = Pattern::new(
            body[..body.len() - 6].to_owned(),
            body[body.len() - 6..].to_owned(),
            false,
            kind,
        )?;
        backend.init(&points, &p)?;
        backend.dispatch_batch()?;
        if backend.retrieve_match()?.is_some() {
            bail!("GPU self-test false positive");
        }
        backend.dispatch_batch()?;
        let Some((_, step)) = backend.retrieve_match()? else {
            bail!("GPU self-test missed known address");
        };
        if step != 7 {
            bail!("GPU self-test point offset mismatch");
        }
    }
    Ok(())
}

#[cfg(all(
    test,
    any(
        feature = "gpu-opencl",
        all(feature = "gpu-metal", target_os = "macos")
    )
))]
pub(crate) fn differential_test(backend: &mut dyn GpuBackend) -> Result<()> {
    use crate::address::{self, AddressType};
    use sha2::{Digest, Sha256};
    let secp = cpu::context()?;
    let mut bases = Vec::new();
    let mut points = Vec::new();
    for lane in 0..LANES {
        // Public, deterministic fixtures, including near the scalar-order boundary.
        let mut bytes: [u8; 32] = Sha256::digest((lane as u32).to_be_bytes()).into();
        if lane == 0 {
            bytes.fill(0);
            bytes[31] = 1;
        }
        if lane == LANES - 1 {
            bytes = cpu::ORDER;
            let mut borrow = 100u16;
            for b in bytes.iter_mut().rev() {
                let sub = (*b as u16).wrapping_sub(borrow);
                *b = sub as u8;
                borrow = u16::from(sub > 255);
            }
        }
        let mut key = secp256k1::SecretKey::from_byte_array(bytes)?;
        let public = secp256k1::PublicKey::from_secret_key(&secp, &key);
        key.non_secure_erase();
        points.extend(point_words(&public));
        bases.push(public);
    }
    for kind in [AddressType::Legacy, AddressType::Bech32] {
        for (lane, offset, insensitive) in [(0, 0, false), (127, 15, true), (255, 31, false)] {
            let mut target = bases[lane];
            for _ in 0..offset {
                target = target.combine(&cpu::generator())?;
            }
            let mut address = address::derive(&target, kind);
            if insensitive && kind == AddressType::Legacy {
                address.make_ascii_uppercase();
            }
            let body = &address[kind.fixed().len()..];
            let p = Pattern::new(
                body[..body.len() - 6].to_owned(),
                body[body.len() - 6..].to_owned(),
                insensitive,
                kind,
            )?;
            backend.init(&points, &p)?;
            for batch in 0..=offset / STEPS {
                backend.dispatch_batch()?;
                let hit = backend.retrieve_match()?;
                if batch < offset / STEPS {
                    assert!(hit.is_none());
                } else {
                    assert_eq!(hit, Some((lane, (offset % STEPS) as u32)));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod chain_tests {
    #[test]
    fn bitcoin_gpu_cannot_silently_search_another_chain() {
        for kind in [
            crate::address::AddressType::Ethereum,
            crate::address::AddressType::Solana,
            crate::address::AddressType::Bnb,
            crate::address::AddressType::Xrp,
            crate::address::AddressType::Tron,
            crate::address::AddressType::Dogecoin,
            crate::address::AddressType::Litecoin,
        ] {
            let p = crate::pattern::Pattern::new("".into(), "".into(), false, kind).unwrap();
            assert!(super::parameters(&p).is_err());
        }
    }
}
