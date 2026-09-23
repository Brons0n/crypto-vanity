use crate::{address, pattern::Pattern};
use anyhow::{anyhow, Context, Result};
use secp256k1::{PublicKey, Secp256k1, SecretKey, Signing};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub const ORDER: [u8; 32] = [
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe,
    0xba, 0xae, 0xdc, 0xe6, 0xaf, 0x48, 0xa0, 0x3b, 0xbf, 0xd2, 0x5e, 0x8c, 0xd0, 0x36, 0x41, 0x41,
];

// Intentionally no Debug/Display on any secret-bearing type.
pub struct Candidate {
    pub secret: Zeroizing<[u8; 32]>,
    pub public: PublicKey,
}

impl Candidate {
    pub fn random<C: Signing>(secp: &Secp256k1<C>) -> Result<Self> {
        let mut bytes = Zeroizing::new([0u8; 32]);
        loop {
            getrandom::fill(&mut *bytes)
                .map_err(|_| anyhow!("OS cryptographic random source failed"))?;
            if let Ok(mut secret) = SecretKey::from_byte_array(*bytes) {
                let public = PublicKey::from_secret_key(secp, &secret);
                secret.non_secure_erase();
                return Ok(Self {
                    secret: bytes,
                    public,
                });
            }
        }
    }

    pub fn advance<C: Signing>(
        &mut self,
        secp: &Secp256k1<C>,
        generator: &PublicKey,
    ) -> Result<()> {
        for b in self.secret.iter_mut().rev() {
            let (next, carry) = b.overflowing_add(1);
            *b = next;
            if !carry {
                break;
            }
        }
        if *self.secret == ORDER {
            // n*G is infinity and scalar zero is invalid: reseed before using it.
            *self = Self::random(secp)?;
        } else {
            self.public = self
                .public
                .combine(generator)
                .context("point addition failed")?;
        }
        Ok(())
    }
}

pub fn context() -> Result<Secp256k1<secp256k1::All>> {
    let mut seed = Zeroizing::new([0u8; 32]);
    getrandom::fill(&mut *seed).map_err(|_| anyhow!("OS cryptographic random source failed"))?;
    let mut secp = Secp256k1::new();
    secp.seeded_randomize(&seed);
    Ok(secp)
}

pub fn generator() -> PublicKey {
    "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798"
        .parse()
        .expect("curve generator")
}

pub struct Run<T = Candidate> {
    pub found: Option<T>,
    pub attempts: u64,
    pub elapsed: Duration,
}

pub(crate) trait Worker {
    type Found: Send;
    fn address(&self, kind: address::AddressType) -> String;
    fn advance(&mut self) -> Result<()>;
    fn into_candidate(self) -> Self::Found;
}
struct SecpWorker {
    secp: Secp256k1<secp256k1::All>,
    generator: PublicKey,
    candidate: Candidate,
}
impl Worker for SecpWorker {
    type Found = Candidate;
    fn address(&self, kind: address::AddressType) -> String {
        address::derive(&self.candidate.public, kind)
    }
    fn advance(&mut self) -> Result<()> {
        self.candidate.advance(&self.secp, &self.generator)
    }
    fn into_candidate(self) -> Candidate {
        self.candidate
    }
}
pub fn run(
    pattern: &Pattern,
    threads: usize,
    benchmark: Option<Duration>,
    cancelled: Arc<AtomicBool>,
    progress: impl FnMut(u64, Duration),
) -> Result<Run> {
    if pattern.kind == address::AddressType::Solana {
        anyhow::bail!("Solana requires the Ed25519 search worker");
    }
    run_workers(pattern, threads, benchmark, cancelled, progress, || {
        let secp = context()?;
        let candidate = Candidate::random(&secp)?;
        Ok(SecpWorker {
            secp,
            generator: generator(),
            candidate,
        })
    })
}

// Local counters avoid per-key contention; cancellation is checked every 128 keys.
pub(crate) fn run_workers<W: Worker>(
    pattern: &Pattern,
    threads: usize,
    benchmark: Option<Duration>,
    cancelled: Arc<AtomicBool>,
    mut progress: impl FnMut(u64, Duration),
    init: impl Fn() -> Result<W> + Sync,
) -> Result<Run<W::Found>> {
    if threads == 0 {
        anyhow::bail!("at least one CPU thread is required");
    }
    let start = Instant::now();
    let stop = AtomicBool::new(false);
    let attempts = AtomicU64::new(0);
    let (tx, rx) = mpsc::channel::<Result<W::Found>>();
    thread::scope(|scope| -> Result<Run<W::Found>> {
        let mut handles = Vec::new();
        for _ in 0..threads {
            let init = &init;
            let tx = tx.clone();
            let stop = &stop;
            let attempts = &attempts;
            let cancelled = &cancelled;
            let worker = thread::Builder::new().spawn_scoped(scope, move || {
                let work = || -> Result<Option<W::Found>> {
                    let mut worker = init()?;
                    while !stop.load(Ordering::Relaxed) && !cancelled.load(Ordering::Relaxed) {
                        if benchmark.is_some_and(|d| start.elapsed() >= d) {
                            break;
                        }
                        let mut count = 0;
                        let mut hit = false;
                        let batch_result = (|| -> Result<()> {
                            for _ in 0..128 {
                                let addr = worker.address(pattern.kind);
                                let matches = pattern.matches(&addr);
                                count += 1;
                                if matches && benchmark.is_none() {
                                    hit = true;
                                    break;
                                }
                                worker.advance()?;
                            }
                            Ok(())
                        })();
                        attempts.fetch_add(count, Ordering::Relaxed);
                        batch_result?;
                        if hit {
                            if !stop.swap(true, Ordering::Relaxed) {
                                return Ok(Some(worker.into_candidate()));
                            }
                            break;
                        }
                    }
                    Ok(None)
                };
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work))
                    .unwrap_or_else(|_| Err(anyhow!("CPU worker panicked")));
                match outcome {
                    Ok(Some(hit)) => {
                        let _ = tx.send(Ok(hit));
                    }
                    Err(e) => {
                        stop.store(true, Ordering::Relaxed);
                        let _ = tx.send(Err(e));
                    }
                    Ok(None) => {}
                }
            });
            match worker {
                Ok(handle) => handles.push(handle),
                Err(e) => {
                    stop.store(true, Ordering::Relaxed);
                    return Err(e).context("could not create CPU worker");
                }
            }
        }
        drop(tx);
        let mut found = None;
        let mut error = None;
        loop {
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(Ok(hit)) => {
                    found = Some(hit);
                    break;
                }
                Ok(Err(e)) => {
                    error = Some(e);
                    break;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    progress(attempts.load(Ordering::Relaxed), start.elapsed())
                }
            }
        }
        stop.store(true, Ordering::Relaxed);
        for handle in handles {
            if handle.join().is_err() {
                error = Some(anyhow!("CPU worker panicked"));
            }
        }
        // An error sent by another worker after the winning result is still significant.
        for message in rx.try_iter() {
            if let Err(e) = message {
                error = Some(e);
            }
        }
        if let Some(e) = error {
            return Err(e);
        }
        Ok(Run {
            found,
            attempts: attempts.load(Ordering::Relaxed),
            elapsed: start.elapsed(),
        })
    })
}

pub fn verify(candidate: &Candidate, pattern: &Pattern) -> Result<String> {
    if pattern.kind == address::AddressType::Solana {
        anyhow::bail!("wrong result key type for Solana");
    }
    let mut secret =
        SecretKey::from_byte_array(*candidate.secret).context("invalid result scalar")?;
    let public = PublicKey::from_secret_key(&context()?, &secret);
    secret.non_secure_erase();
    let address = address::derive(&public, pattern.kind);
    if public != candidate.public || !pattern.matches(&address) {
        return Err(anyhow!(
            "result verification failed; no private key will be printed"
        ));
    }
    Ok(address)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incremental_points_match_scalar_multiplication() {
        let secp = context().unwrap();
        let mut c = Candidate::random(&secp).unwrap();
        for _ in 0..512 {
            let mut key = SecretKey::from_byte_array(*c.secret).unwrap();
            assert_eq!(c.public, PublicKey::from_secret_key(&secp, &key));
            key.non_secure_erase();
            c.advance(&secp, &generator()).unwrap();
        }
        let mut last = ORDER;
        last[31] -= 1;
        let mut key = SecretKey::from_byte_array(last).unwrap();
        c = Candidate {
            secret: Zeroizing::new(last),
            public: PublicKey::from_secret_key(&secp, &key),
        };
        key.non_secure_erase();
        c.advance(&secp, &generator()).unwrap();
        assert!(SecretKey::from_byte_array(*c.secret).is_ok());
    }
    #[test]
    fn threaded_search_and_cancellation() {
        let p = Pattern::new("".into(), "".into(), false, address::AddressType::Legacy).unwrap();
        let r = run(&p, 4, None, Arc::new(AtomicBool::new(false)), |_, _| {}).unwrap();
        assert!(r.attempts >= 1);
        verify(&r.found.unwrap(), &p).unwrap();
        let r = run(&p, 2, None, Arc::new(AtomicBool::new(true)), |_, _| {}).unwrap();
        assert!(r.found.is_none());
        assert_eq!(r.attempts, 0);
    }
}
