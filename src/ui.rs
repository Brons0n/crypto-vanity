//! Line-based terminal interface: works without raw mode or a GPU on every OS.
//! Prompts stay on stderr; secret material is only printed by the final result path.
use crate::{estimate, parse_threads, AddressType, Args, Chain, Pattern};
use anyhow::{bail, Context, Result};
use std::{
    io::{self, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};

const STRONG: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

pub fn configure(args: &mut Args) -> Result<bool> {
    let title = "VANITY GENERATOR";
    anstream::eprintln!(
        "\n{STRONG}  ╭────────────────────────────────────────────────╮\n  │{title:^48}│\n  ╰────────────────────────────────────────────────╯{RESET}\n\n  {DIM}OFFLINE  /  8 NETWORKS  /  KEYS STAY ON THIS DEVICE{RESET}\n  Press Enter to use the suggested choice. 0 exits a menu.\n"
    );
    if !select_networks(args, false)? {
        return Ok(false);
    }
    if args.kind()? == AddressType::Bech32 {
        args.case_insensitive = false;
    }
    let mut speed = SpeedPreview::new();
    simple_pattern(args, &mut speed)?;
    loop {
        review(args, &mut speed)?;
        let first = if args.estimate_only {
            "Measure speed & estimate"
        } else {
            "Create my address (recommended)"
        };
        let Some(action) = choose(
            "Press Enter to continue",
            &[
                first,
                "Check how long it might take",
                "More options",
                "Change text",
                "Change network",
            ],
            1,
        )?
        else {
            return Ok(false);
        };
        match action {
            1 | 2 => {
                args.estimate_only |= action == 2;
                section(
                    "03",
                    if args.estimate_only {
                        "MEASURING"
                    } else {
                        "SEARCHING"
                    },
                );
                return Ok(true);
            }
            3 => {
                let Some(option) = choose(
                    "More options",
                    &[
                        "Text position and letter case",
                        "Speed and saving to a file",
                    ],
                    1,
                )?
                else {
                    return Ok(false);
                };
                if !(if option == 1 {
                    pattern(args)?
                } else {
                    settings(args)?
                }) {
                    return Ok(false);
                }
            }
            4 => {
                simple_pattern(args, &mut speed)?;
            }
            5 => {
                if !select_networks(args, true)? {
                    return Ok(false);
                }
                args.prefix = None;
                args.suffix.clear();
                if args.kind()? == AddressType::Bech32 {
                    args.case_insensitive = false;
                }
                simple_pattern(args, &mut speed)?;
            }
            _ => unreachable!(),
        }
    }
}

fn select_networks(args: &mut Args, address_style: bool) -> Result<bool> {
    section("01", "NETWORKS");
    let networks = [
        Chain::Bitcoin,
        Chain::Ethereum,
        Chain::Solana,
        Chain::Bnb,
        Chain::Xrp,
        Chain::Tron,
        Chain::Dogecoin,
        Chain::Litecoin,
    ];
    for (i, chain) in networks.iter().enumerate() {
        anstream::eprintln!(
            "  {}  {}",
            i + 1,
            args.for_chain(*chain).kind()?.chain_name()
        );
    }
    anstream::eprintln!(
        "  {DIM}0  Exit{RESET}\n  Select one or more, e.g. 1,2,3. Runs in that order."
    );
    loop {
        let text = prompt(&format!(
            "Networks [Enter: {}]",
            args.for_chain(args.chain).kind()?.chain_name()
        ))?;
        if text == "0" {
            return Ok(false);
        }
        let indices = if text.is_empty() {
            Ok(vec![args.chain as usize + 1])
        } else {
            text.split(',')
                .map(|s| s.trim().parse::<usize>())
                .collect::<std::result::Result<Vec<_>, _>>()
        };
        let Ok(indices) = indices else {
            anstream::eprintln!("  Use numbers 1–8 separated by commas.");
            continue;
        };
        if indices.iter().any(|n| !(1..=8).contains(n)) {
            anstream::eprintln!("  Use numbers 1–8 separated by commas.");
            continue;
        }
        args.queue.clear();
        for index in indices {
            let chain = networks[index - 1];
            if !args.queue.contains(&chain) {
                args.queue.push(chain);
            }
        }
        args.chain = args.queue[0];
        break;
    }
    if matches!(args.chain, Chain::Bitcoin) && address_style {
        let default = if args.address_type == Some(AddressType::Bech32) {
            2
        } else {
            1
        };
        let Some(format) = choose(
            "Address style",
            &["Classic  ·  1…", "Native SegWit  ·  bc1q…"],
            default,
        )?
        else {
            return Ok(false);
        };
        args.address_type = Some(if format == 1 {
            AddressType::Legacy
        } else {
            AddressType::Bech32
        });
    } else if args.chain != Chain::Bitcoin {
        args.address_type = None;
    }
    if !args.kind()?.is_bitcoin() {
        args.gpu = false;
    }
    Ok(true)
}

struct SpeedPreview {
    measured: Option<(AddressType, usize, f64)>,
}

impl SpeedPreview {
    fn new() -> Self {
        Self { measured: None }
    }

    fn rate(&mut self, args: &Args) -> Result<f64> {
        let kind = args.kind()?;
        if let Some((previous_kind, threads, rate)) = self.measured {
            if previous_kind == kind && threads == args.threads {
                return Ok(rate);
            }
        }
        anstream::eprintln!("  Measuring speed…");
        let pattern = Pattern::new(String::new(), String::new(), false, kind)?;
        let result = crate::search::run(
            &pattern,
            args.threads,
            Some(Duration::from_secs(1)),
            Arc::new(AtomicBool::new(false)),
            |_, _| {},
        )?;
        let rate = result.attempts as f64 / result.elapsed.as_secs_f64();
        if !rate.is_finite() || rate <= 0.0 {
            bail!("could not measure search speed");
        }
        self.measured = Some((kind, args.threads, rate));
        Ok(rate)
    }
}

fn short_eta(seconds: f64) -> String {
    if seconds < 1.0 {
        "under 1 sec".into()
    } else {
        estimate::duration(seconds)
    }
}

fn length_estimates(args: &Args, speed: &mut SpeedPreview) -> Result<()> {
    let rate = speed.rate(args)?;
    let kind = args.kind()?;
    let pattern = Pattern::new(String::new(), String::new(), args.case_insensitive, kind)?;
    let (min, max) = kind
        .alphabet()
        .iter()
        .map(|&c| pattern.character_cost(c))
        .fold((f64::INFINITY, 0.0_f64), |(min, max), n| {
            (min.min(n), max.max(n))
        });
    anstream::eprintln!(
        "\n  {STRONG}Characters     Rough ETA · {}{RESET}",
        kind.chain_name()
    );
    for count in 1..=8 {
        let low = short_eta(min.powi(count) / rate);
        let high = short_eta(max.powi(count) / rate);
        let eta = if low == high {
            low
        } else {
            format!("{low} – {high}")
        };
        anstream::eprintln!("  {count:<14} {eta}");
    }
    anstream::eprintln!(
        "  {DIM}Based on this computer's CPU. Characters affect difficulty; times vary.{RESET}"
    );
    Ok(())
}

fn validate_text(args: &Args, prefix: &str, suffix: &str, insensitive: bool) -> Result<()> {
    for chain in args.selected_chains() {
        let kind = args.for_chain(chain).kind()?;
        Pattern::new(prefix.into(), suffix.into(), insensitive, kind)
            .with_context(|| format!("{} cannot use this text", kind.chain_name()))?;
    }
    Ok(())
}

fn simple_pattern(args: &mut Args, speed: &mut SpeedPreview) -> Result<()> {
    section("02", "YOUR TEXT");
    length_estimates(args, speed)?;
    loop {
        let text = prompt("Text")?;
        if text.is_empty() {
            anstream::eprintln!("  Enter at least one character.");
            continue;
        }
        match validate_text(args, &text, &args.suffix, args.case_insensitive) {
            Ok(_) => {
                args.prefix = Some(text);
                return Ok(());
            }
            Err(error) => anstream::eprintln!("  Please try different text: {error:#}"),
        }
    }
}

fn pattern(args: &mut Args) -> Result<bool> {
    let kind = args.kind()?;
    section("02", "MAKE IT YOURS");
    if kind.fixed().is_empty() {
        anstream::eprintln!("  Your pattern starts at the first address character.");
    } else {
        anstream::eprintln!(
            "  The network adds {}. Your prefix comes just after it.",
            kind.fixed()
        );
    }
    if kind.is_evm() {
        anstream::eprintln!("  Use hexadecimal text: 0–9 and a–f. Try b, cafe, or dead.");
    } else if kind == AddressType::Bech32 {
        anstream::eprintln!("  Use lowercase bech32 text (no 1, b, i, or o). Try cat.");
    } else {
        anstream::eprintln!("  Use Base58 letters and digits (no 0, O, I, or l). Try b.");
    }
    let Some(mode) = choose(
        "Where should it appear?",
        &["At the start", "At the end", "Both start and end"],
        1,
    )?
    else {
        return Ok(false);
    };
    loop {
        let prefix = if mode != 2 {
            prompt("Starting text")?
        } else {
            String::new()
        };
        let suffix = if mode != 1 {
            prompt("Ending text")?
        } else {
            String::new()
        };
        if prefix.is_empty() && suffix.is_empty() {
            anstream::eprintln!("  Please enter at least one character to match.");
            continue;
        }
        let insensitive = if kind == AddressType::Bech32 {
            false
        } else {
            !yes_no("Match uppercase/lowercase exactly?", !args.case_insensitive)?
        };
        match validate_text(args, &prefix, &suffix, insensitive) {
            Ok(_) => {
                args.prefix = Some(prefix);
                args.suffix = suffix;
                args.case_insensitive = insensitive;
                return Ok(true);
            }
            Err(error) => {
                anstream::eprintln!("\n  {STRONG}Try another pattern:{RESET} {error:#}\n")
            }
        }
    }
}

fn review(args: &Args, speed: &mut SpeedPreview) -> Result<()> {
    let kind = args.kind()?;
    let prefix = args.prefix.as_deref().unwrap_or_default();
    anstream::eprintln!(
        "\n  {STRONG}YOUR SEARCH{RESET}\n  ──────────────────────────────────────────────────"
    );
    let names = args
        .selected_chains()
        .iter()
        .map(|chain| args.for_chain(*chain).kind().map(|k| k.chain_name()))
        .collect::<Result<Vec<_>>>()?;
    anstream::eprintln!("  Network    {}", names.join(" → "));
    if names.len() > 1 {
        anstream::eprintln!("  One result per network, using the same text.");
    }
    let pattern = Pattern::new(
        prefix.into(),
        args.suffix.clone(),
        args.case_insensitive,
        kind,
    )?;
    let eta = estimate::Estimate::new(&pattern).mean_seconds(speed.rate(args)?);
    anstream::eprintln!(
        "  Rough ETA  {} (first network, CPU estimate)",
        short_eta(eta)
    );
    anstream::eprintln!(
        "  Preview    {STRONG}{}{prefix}…{}{RESET}",
        kind.fixed(),
        args.suffix
    );
    anstream::eprintln!(
        "  Letters    {}",
        if args.case_insensitive {
            "Uppercase or lowercase both work"
        } else {
            "Match exactly as typed"
        }
    );
    anstream::eprintln!(
        "  Compute    {} · {} {}",
        if args.gpu {
            "GPU with CPU fallback"
        } else {
            "CPU"
        },
        args.threads,
        if args.threads == 1 {
            "thread"
        } else {
            "threads"
        }
    );
    anstream::eprintln!(
        "  Save       {}",
        args.output_file
            .as_ref()
            .map_or_else(|| "Show result only".into(), |p| p.display().to_string())
    );
    if kind == AddressType::Xrp {
        anstream::eprintln!("  XRP keys require software that accepts direct private keys.");
    }
    anstream::eprintln!("  ──────────────────────────────────────────────────\n  Speed and estimated time are measured before the search.\n");
    Ok(())
}

fn settings(args: &mut Args) -> Result<bool> {
    section("+", "SETTINGS");
    loop {
        let text = prompt(&format!("CPU threads [Enter keeps {}]", args.threads))?;
        if text.is_empty() {
            break;
        }
        match parse_threads(&text) {
            Ok(value) => {
                args.threads = value;
                break;
            }
            Err(error) => anstream::eprintln!("  {error}"),
        }
    }
    if args.kind()?.is_bitcoin() {
        let Some(engine) = choose(
            "Compute",
            &[
                "CPU · always available",
                "GPU · falls back to CPU if unavailable",
            ],
            if args.gpu { 2 } else { 1 },
        )?
        else {
            return Ok(false);
        };
        args.gpu = engine == 2;
    }
    if yes_no(
        "Also save the final key report to a file?",
        args.output_file.is_some(),
    )? {
        loop {
            let path = prompt("New file path")?;
            if path.is_empty() {
                anstream::eprintln!("  Enter a new file path.");
            } else if std::path::Path::new(&path).exists() {
                anstream::eprintln!(
                    "  That file exists. Choose a new path; existing files are never overwritten."
                );
            } else {
                args.output_file = Some(path.into());
                break;
            }
        }
    } else {
        args.output_file = None;
    }
    Ok(true)
}

pub fn confirm_long_search(seconds: f64, hours: f64, cancelled: Arc<AtomicBool>) -> Result<bool> {
    anstream::eprintln!("\n  {STRONG}LONG SEARCH{RESET}\n  Estimated mean: {}. Your limit is {hours:.2} hours.\n  Actual completion may take longer.", estimate::duration(seconds));
    // The search's Ctrl-C handler is installed by this point. Avoid blocking the
    // main thread in stdin, so cancellation still works while this prompt is open.
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(yes_no("Start this search anyway?", false));
    });
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Ok(false);
        }
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(answer) => return answer,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("confirmation prompt stopped"),
        }
    }
}

pub fn success() {
    anstream::eprintln!("\n  {STRONG}✓ MATCH FOUND{RESET}  ·  Key and address verified\n  Your complete result follows. Keep the private key secure.\n");
}

fn section(step: &str, title: &str) {
    anstream::eprintln!("\n  {STRONG}{step}{RESET}  {STRONG}{title}{RESET}\n");
}

fn choose(label: &str, options: &[&str], default: usize) -> Result<Option<usize>> {
    for (i, option) in options.iter().enumerate() {
        anstream::eprintln!("  {STRONG}{:>2}{RESET}  {option}", i + 1);
    }
    anstream::eprintln!("  {DIM} 0  Exit{RESET}");
    loop {
        let value = prompt(&format!("{label} [Enter: {}]", options[default - 1]))?;
        let parsed = if value.is_empty() {
            Some(default)
        } else {
            value.parse::<usize>().ok()
        };
        match parsed {
            Some(0) => return Ok(None),
            Some(n) if n <= options.len() => return Ok(Some(n)),
            _ => anstream::eprintln!("  Choose a number from 0 to {}.", options.len()),
        }
    }
}

fn yes_no(label: &str, default: bool) -> Result<bool> {
    loop {
        let text = prompt(&format!(
            "{label} {}",
            if default { "[Y/n]" } else { "[y/N]" }
        ))?;
        match text.to_ascii_lowercase().as_str() {
            "" => return Ok(default),
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => anstream::eprintln!("  Enter y or n."),
        }
    }
}

fn prompt(label: &str) -> Result<String> {
    anstream::eprint!("\n  {label}\n  {STRONG}›{RESET} ");
    anstream::stderr().flush().context("displaying prompt")?;
    let mut line = String::new();
    if io::stdin()
        .read_line(&mut line)
        .context("reading selection")?
        == 0
    {
        bail!("input closed; search cancelled");
    }
    Ok(line.trim().to_owned())
}
