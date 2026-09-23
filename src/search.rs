use crate::{
    address::{self, AddressType},
    cpu,
    pattern::Pattern,
    solana,
};
use anyhow::Result;
use std::{
    fmt::Write,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use zeroize::Zeroizing;

pub enum Found {
    Secp(cpu::Candidate),
    Solana(solana::Candidate),
}
pub fn from_secp(run: cpu::Run) -> cpu::Run<Found> {
    cpu::Run {
        found: run.found.map(Found::Secp),
        attempts: run.attempts,
        elapsed: run.elapsed,
    }
}
pub fn run(
    pattern: &Pattern,
    threads: usize,
    benchmark: Option<Duration>,
    cancelled: Arc<AtomicBool>,
    progress: impl FnMut(u64, Duration),
) -> Result<cpu::Run<Found>> {
    if pattern.kind == AddressType::Solana {
        let r = solana::run(pattern, threads, benchmark, cancelled, progress)?;
        Ok(cpu::Run {
            found: r.found.map(Found::Solana),
            attempts: r.attempts,
            elapsed: r.elapsed,
        })
    } else {
        cpu::run(pattern, threads, benchmark, cancelled, progress).map(from_secp)
    }
}

pub fn format_result(
    candidate: &Found,
    pattern: &Pattern,
    attempts: u64,
    elapsed: Duration,
) -> Result<Zeroizing<String>> {
    let address = match candidate {
        Found::Secp(c) => cpu::verify(c, pattern)?,
        Found::Solana(c) => solana::verify(c, pattern)?,
    };
    let mut output = Zeroizing::new(String::with_capacity(1024));
    writeln!(
        output,
        "Chain: {}\nAddress: {address}",
        pattern.kind.chain_name()
    )?;
    match candidate {
        Found::Secp(c) => {
            if let Some(version) = pattern.kind.wif_version() {
                writeln!(
                    output,
                    "Private key (WIF compressed): {}",
                    *address::wif_with_version(&c.secret, version)
                )?;
            }
            writeln!(output, "Private key (hex): {}", *address::hex(&*c.secret))?;
            if pattern.kind == AddressType::Xrp {
                // XRPL keypair APIs mark a direct secp256k1 secret with a 00 byte.
                // This is not an XRPL family seed and must not be labeled as one.
                writeln!(
                    output,
                    "Private key (XRPL hex): 00{}",
                    *address::hex(&*c.secret)
                )?;
            }
            if pattern.kind.is_evm() || pattern.kind == AddressType::Tron {
                writeln!(
                    output,
                    "Public key (hex uncompressed): {}",
                    *address::hex(&c.public.serialize_uncompressed())
                )?;
            } else {
                writeln!(
                    output,
                    "Public key (hex compressed): {}",
                    *address::hex(&c.public.serialize())
                )?;
            }
        }
        Found::Solana(c) => {
            let bytes = c.keypair_bytes();
            let mut encoded = Zeroizing::new(String::with_capacity(88));
            bs58::encode(&bytes[..])
                .onto(&mut *encoded)
                .expect("String is growable");
            writeln!(
                output,
                "Private seed (hex, 32 bytes): {}",
                *address::hex(&*c.seed)
            )?;
            writeln!(output, "Private key (base58, 64 bytes): {}", *encoded)?;
            writeln!(
                output,
                "Public key (hex Ed25519): {}",
                *address::hex(&c.public)
            )?;
        }
    }
    write!(
        output,
        "Total attempts: {attempts}\nElapsed time: {:.6} seconds\nAverage keys/sec: {:.2}\n",
        elapsed.as_secs_f64(),
        attempts as f64 / elapsed.as_secs_f64()
    )?;
    Ok(output)
}
