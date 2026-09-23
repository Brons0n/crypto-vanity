use anyhow::{bail, Context, Result};
use bitcoin_vanity::{
    address::AddressType,
    estimate::{self, Estimate},
    gpu,
    pattern::Pattern,
    search,
};
use clap::{Parser, ValueEnum};
use std::{
    fs::OpenOptions,
    io::{self, IsTerminal, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

mod ui;

#[derive(Parser, Clone)]
#[command(
    name = "vanitybtc",
    version,
    about = "Offline vanity keypair generator for Bitcoin, Ethereum, Solana, BNB, XRP, TRON, Dogecoin, and Litecoin"
)]
struct Args {
    #[arg(skip)]
    queue: Vec<Chain>,
    /// Guided network and vanity-pattern selection (also opens with no arguments)
    #[arg(short = 'i', long)]
    interactive: bool,
    /// Network key/address scheme (Bitcoin remains the default)
    #[arg(long, value_enum, default_value = "bitcoin")]
    chain: Chain,
    /// Prefix after 1 / bc1q / 0x / r / T / D / L; Solana starts at the first character
    #[arg(long)]
    prefix: Option<String>,
    #[arg(long, default_value = "")]
    suffix: String,
    #[arg(long)]
    case_insensitive: bool,
    /// Bitcoin only: legacy (default) or bech32
    #[arg(long, value_parser = parse_bitcoin_type)]
    address_type: Option<AddressType>,
    #[arg(long, default_value_t = std::thread::available_parallelism().map_or(1, usize::from), value_parser = parse_threads)]
    threads: usize,
    /// Also write the final result to a new file (refuses to overwrite)
    #[arg(long)]
    output_file: Option<PathBuf>,
    #[arg(long)]
    estimate_only: bool,
    /// Require --confirm when the estimated mean exceeds this many hours
    #[arg(long, default_value = "24", value_parser = parse_hours)]
    max_eta_hours: f64,
    #[arg(long)]
    confirm: bool,
    /// Use a compiled GPU backend, with CPU fallback on initialization/runtime errors
    #[arg(long)]
    gpu: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Chain {
    #[value(alias = "btc")]
    Bitcoin,
    #[value(alias = "eth")]
    Ethereum,
    #[value(alias = "sol")]
    Solana,
    /// BNB Smart Chain (EVM, 0x addresses)
    #[value(alias = "bsc")]
    Bnb,
    /// XRP Ledger classic address (secp256k1)
    #[value(alias = "ripple")]
    Xrp,
    #[value(alias = "trx")]
    Tron,
    #[value(alias = "doge")]
    Dogecoin,
    #[value(alias = "ltc")]
    Litecoin,
}
impl Args {
    fn selected_chains(&self) -> Vec<Chain> {
        if self.queue.is_empty() {
            vec![self.chain]
        } else {
            self.queue.clone()
        }
    }
    fn for_chain(&self, chain: Chain) -> Self {
        let mut job = self.clone();
        job.chain = chain;
        job.queue.clear();
        if chain != Chain::Bitcoin {
            job.address_type = None;
        }
        job
    }

    fn kind(&self) -> Result<AddressType> {
        if !matches!(self.chain, Chain::Bitcoin) && self.address_type.is_some() {
            bail!("--address-type is only valid with --chain bitcoin");
        }
        Ok(match self.chain {
            Chain::Bitcoin => self.address_type.unwrap_or(AddressType::Legacy),
            Chain::Ethereum => AddressType::Ethereum,
            Chain::Solana => AddressType::Solana,
            Chain::Bnb => AddressType::Bnb,
            Chain::Xrp => AddressType::Xrp,
            Chain::Tron => AddressType::Tron,
            Chain::Dogecoin => AddressType::Dogecoin,
            Chain::Litecoin => AddressType::Litecoin,
        })
    }
}
fn parse_bitcoin_type(s: &str) -> std::result::Result<AddressType, String> {
    match s {
        "legacy" => Ok(AddressType::Legacy),
        "bech32" => Ok(AddressType::Bech32),
        _ => Err("address type must be legacy or bech32 (Bitcoin only)".into()),
    }
}

fn parse_threads(s: &str) -> std::result::Result<usize, String> {
    s.parse::<usize>()
        .ok()
        .filter(|n| *n > 0 && *n <= 4096)
        .ok_or_else(|| "threads must be between 1 and 4096".into())
}
fn parse_hours(s: &str) -> std::result::Result<f64, String> {
    s.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .ok_or_else(|| "hours must be a finite nonnegative number".into())
}

fn main() {
    if let Err(e) = execute() {
        eprintln!("Error: {e:#}");
        std::process::exit(1);
    }
}

fn execute() -> Result<()> {
    let mut args = Args::parse();
    if std::env::args_os().len() == 1 && io::stdin().is_terminal() {
        args.interactive = true;
    }
    if args.interactive && !ui::configure(&mut args)? {
        return Ok(());
    }
    args.kind()?;
    // Validate the whole queue before starting or opening an output file.
    for chain in args.selected_chains() {
        let job = args.for_chain(chain);
        let prefix = args.prefix.as_ref().context(
            "choose a pattern with --prefix, or open the guided interface with --interactive",
        )?;
        Pattern::new(
            prefix.clone(),
            job.suffix.clone(),
            job.case_insensitive,
            job.kind()?,
        )?;
    }
    if !args.estimate_only && args.output_file.as_ref().is_some_and(|p| p.exists()) {
        bail!("output file already exists; choose a new path");
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = cancelled.clone();
    ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))
        .context("installing interrupt handler")?;
    let chains = args.selected_chains();
    let mut file = None;
    for (index, chain) in chains.iter().enumerate() {
        if cancelled.load(Ordering::Relaxed) {
            bail!("queue cancelled");
        }
        let job = args.for_chain(*chain);
        if chains.len() > 1 {
            eprintln!(
                "\n[{}/{}] {}",
                index + 1,
                chains.len(),
                job.kind()?.chain_name()
            );
        }
        execute_search(&job, cancelled.clone(), &mut file)?;
    }
    Ok(())
}

fn execute_search(
    args: &Args,
    cancelled: Arc<AtomicBool>,
    file: &mut Option<std::fs::File>,
) -> Result<()> {
    let kind = args.kind()?;
    let prefix = args.prefix.as_ref().context(
        "choose a pattern with --prefix, or open the guided interface with --interactive",
    )?;
    let pattern = Pattern::new(
        prefix.clone(),
        args.suffix.clone(),
        args.case_insensitive,
        kind,
    )?;
    let estimate = Estimate::new(&pattern);
    eprintln!("Approximate search space: {:.3e} attempts", estimate.space);
    eprintln!(
        "Half-space estimate: {:.3e} attempts; statistical mean: {:.3e}",
        estimate.half_space, estimate.space
    );
    let mut backend = if args.gpu && !kind.is_bitcoin() {
        eprintln!(
            "WARNING: GPU backends currently support Bitcoin only. Falling back to CPU for {}.",
            kind.chain_name()
        );
        None
    } else if args.gpu {
        match gpu::select() {
            Ok(device) => {
                eprintln!("Using {} (startup verification passed)", device.name());
                Some(device)
            }
            Err(e) => {
                eprintln!("WARNING: GPU unavailable: {e:#}. Falling back to CPU.");
                None
            }
        }
    } else {
        None
    };
    let rate = benchmark(&mut backend, &pattern, args.threads, cancelled.clone())?;
    let eta = estimate.mean_seconds(rate);
    eprintln!(
        "Measured: {rate:.0} keys/sec | half-space ETA: {} | mean ETA: {}",
        estimate::duration(eta / 2.0),
        estimate::duration(eta)
    );
    eprintln!("Rough estimate: Base58 prefixes are biased; checksums constrain long patterns. ETA is not a deadline.");
    if args.estimate_only {
        return Ok(());
    }
    approve_search(eta, args, cancelled.clone())?;
    // Validate/reserve the explicitly requested output before investing in a long search.
    if file.is_none() {
        if let Some(path) = &args.output_file {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            *file = Some(
                options
                    .open(path)
                    .context("creating requested output file")?,
            );
        }
    }
    let mut progress = Progress::new(&estimate);
    let result = if let Some(device) = backend.as_mut() {
        match gpu::run(
            device.as_mut(),
            &pattern,
            None,
            cancelled.clone(),
            |a, e| progress.show(a, e),
        )
        .map(search::from_secp)
        {
            Ok(result) => result,
            Err(e) => {
                eprintln!(
                    "\nWARNING: GPU failed: {e:#}. Restarting on CPU with fresh counters and keys."
                );
                backend = None;
                let rate = benchmark(&mut backend, &pattern, args.threads, cancelled.clone())?;
                approve_search(estimate.mean_seconds(rate), args, cancelled.clone())?;
                progress = Progress::new(&estimate);
                search::run(&pattern, args.threads, None, cancelled.clone(), |a, e| {
                    progress.show(a, e)
                })?
            }
        }
    } else {
        search::run(&pattern, args.threads, None, cancelled, |a, e| {
            progress.show(a, e)
        })?
    };
    if progress.terminal {
        eprintln!();
    }
    let Some(candidate) = result.found else {
        bail!("cancelled after {} attempts", result.attempts);
    };
    let output = search::format_result(&candidate, &pattern, result.attempts, result.elapsed)?;
    if args.interactive {
        ui::success();
    }
    // Print the recoverable result even if a requested file write fails.
    let file_result = file
        .as_mut()
        .map(|f| f.write_all(output.as_bytes()).and_then(|_| f.sync_all()));
    io::stdout()
        .lock()
        .write_all(output.as_bytes())
        .context("writing final result to stdout")?;
    if let Some(r) = file_result {
        r.context("writing requested output file; final result was printed to stdout")?;
    }
    Ok(())
}

fn approve_search(eta: f64, args: &Args, cancelled: Arc<AtomicBool>) -> Result<()> {
    if args.interactive && !args.confirm && eta / 3600.0 > args.max_eta_hours {
        if ui::confirm_long_search(eta, args.max_eta_hours, cancelled)? {
            return Ok(());
        }
        bail!("search cancelled; no key generated");
    }
    guard(eta, args.max_eta_hours, args.confirm)
}

fn guard(eta: f64, max_hours: f64, confirm: bool) -> Result<()> {
    if eta / 3600.0 > max_hours && !confirm {
        bail!("WARNING: estimated mean exceeds {max_hours:.2} hours. Pass --confirm to start this search anyway");
    }
    Ok(())
}

fn benchmark(
    backend: &mut Option<Box<dyn gpu::GpuBackend>>,
    pattern: &Pattern,
    threads: usize,
    cancelled: Arc<AtomicBool>,
) -> Result<f64> {
    let duration = Some(Duration::from_millis(1500));
    let mut bench = None;
    if let Some(device) = backend.as_mut() {
        eprintln!(
            "Benchmarking {} for approximately 1.5 seconds…",
            device.name()
        );
        match gpu::run(
            device.as_mut(),
            pattern,
            duration,
            cancelled.clone(),
            |_, _| {},
        )
        .map(search::from_secp)
        {
            Ok(r) => bench = Some(r),
            Err(e) => {
                eprintln!("WARNING: GPU benchmark failed: {e:#}. Falling back to CPU.");
                *backend = None;
            }
        }
    }
    let bench = if let Some(bench) = bench {
        bench
    } else {
        eprintln!("Benchmarking {threads} CPU threads for 1.5 seconds…");
        search::run(pattern, threads, duration, cancelled.clone(), |_, _| {})?
    };
    if cancelled.load(Ordering::Relaxed) {
        bail!("cancelled");
    }
    let rate = bench.attempts as f64 / bench.elapsed.as_secs_f64();
    if rate <= 0.0 {
        bail!("benchmark did not measure any keys");
    }
    Ok(rate)
}

struct Progress<'a> {
    terminal: bool,
    previous: (u64, Duration),
    width: usize,
    estimate: &'a Estimate,
}
impl<'a> Progress<'a> {
    fn new(estimate: &'a Estimate) -> Self {
        Self {
            terminal: io::stderr().is_terminal(),
            previous: (0, Duration::ZERO),
            width: 0,
            estimate,
        }
    }
    fn show(&mut self, attempts: u64, elapsed: Duration) {
        if !self.terminal {
            return;
        }
        let rate = (attempts - self.previous.0) as f64 / (elapsed - self.previous.1).as_secs_f64();
        let line = format!(
            "Elapsed {} | attempts {attempts} | {rate:.0} keys/sec | rough remaining mean {}",
            estimate::duration(elapsed.as_secs_f64()),
            estimate::duration(self.estimate.mean_seconds(rate))
        );
        self.width = self.width.max(line.len());
        // Carriage return and padding also work on Windows consoles without ANSI mode.
        eprint!("\r{line:<width$}", width = self.width);
        let _ = io::stderr().flush();
        self.previous = (attempts, elapsed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct FailedGpu;
    impl gpu::GpuBackend for FailedGpu {
        fn name(&self) -> &str {
            "test device"
        }
        fn init(&mut self, _: &[u32], _: &Pattern) -> Result<()> {
            bail!("simulated missing driver")
        }
        fn dispatch_batch(&mut self) -> Result<u64> {
            unreachable!()
        }
        fn retrieve_match(&mut self) -> Result<Option<(usize, u32)>> {
            unreachable!()
        }
    }
    #[test]
    fn failed_gpu_benchmark_reverts_to_cpu() {
        let mut backend: Option<Box<dyn gpu::GpuBackend>> = Some(Box::new(FailedGpu));
        let p = Pattern::new("".into(), "".into(), false, AddressType::Legacy).unwrap();
        let rate = benchmark(&mut backend, &p, 1, Arc::new(AtomicBool::new(false))).unwrap();
        assert!(backend.is_none());
        assert!(rate > 0.0);
        assert!(guard(3601.0, 1.0, false).is_err());
        assert!(guard(3601.0, 1.0, true).is_ok());
    }
}
