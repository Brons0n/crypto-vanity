use std::{
    io::Write,
    process::{Command, Output, Stdio},
};

fn session(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_vanitybtc"))
        .args(args)
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn field<'a>(text: &'a str, label: &str) -> &'a str {
    text.lines()
        .find_map(|line| line.strip_prefix(label))
        .unwrap()
}

#[test]
fn interactive_search_returns_verified_private_keys() {
    for (style, input, prefix) in [("legacy", "\nb\n\n", "b"), ("bech32", "1\nq\n\n", "q")] {
        let mut args = vec!["--interactive", "--threads", "1"];
        if style == "bech32" {
            args.extend(["--address-type", "bech32"]);
        }
        let out = session(&args, input);
        assert!(out.status.success());
        let text = zeroize::Zeroizing::new(String::from_utf8(out.stdout).unwrap());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("MATCH FOUND"));
        assert!(stderr.contains("Match exactly as typed"));
        assert!(!stderr.contains('\x1b'));
        assert_eq!(field(&text, "Chain: "), "bitcoin");
        let wif = field(&text, "Private key (WIF compressed): ");
        let raw = field(&text, "Private key (hex): ");
        let mut key = bitcoin::PrivateKey::from_wif(wif).unwrap();
        assert!(key.compressed);
        assert!(key.inner.display_secret().to_string() == raw);
        let public = key.public_key(&bitcoin::secp256k1::Secp256k1::new());
        key.inner.non_secure_erase();
        assert_eq!(
            public.to_string(),
            field(&text, "Public key (hex compressed): ")
        );
        let expected = if style == "legacy" {
            bitcoin::Address::p2pkh(public, bitcoin::Network::Bitcoin)
        } else {
            bitcoin::Address::p2wpkh(
                &bitcoin::CompressedPublicKey(public.inner),
                bitcoin::Network::Bitcoin,
            )
        };
        let address = field(&text, "Address: ");
        assert_eq!(expected.to_string(), address);
        let fixed = if style == "legacy" { 1 } else { 4 };
        assert!(address[fixed..].starts_with(prefix));
        if style == "legacy" {
            assert!(stderr.contains("NETWORKS"));
            assert!(!stderr.contains("Address style"));
            assert!(!stderr.contains("Where should it appear?"));
        }
        assert!(!stderr.contains(wif));
        assert!(!stderr.contains(raw));
        assert!(field(&text, "Total attempts: ").parse::<u64>().unwrap() > 0);
        // Public evidence only; never include generated secrets in test logs.
        eprintln!("Interactive {style} check: {address}; WIF and raw private key both present and independently verified.");
    }
}

#[test]
fn every_network_can_be_selected_and_cancelled_without_searching() {
    for (choice, name, fixed) in [
        (1, "bitcoin", "1"),
        (2, "ethereum", "0x"),
        (3, "solana", ""),
        (4, "bnb", "0x"),
        (5, "xrp", "r"),
        (6, "tron", "T"),
        (7, "dogecoin", "D"),
        (8, "litecoin", "L"),
    ] {
        let prefix = if matches!(choice, 6 | 7) { "B" } else { "b" };
        let input = format!("{choice}\n{prefix}\n3\n1\n2\nb\nn\n0\n");
        let out = session(&["--interactive"], &input);
        assert!(out.status.success());
        assert!(out.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains(&format!("Network    {name}")));
        assert!(stderr.contains(&format!("Preview    {fixed}…b")));
        assert!(!stderr.contains("Benchmarking"));
    }
}

#[test]
fn invalid_patterns_retry_and_estimate_only_never_emits_keys() {
    // Reject mixed-case bech32, then accept a new pattern without restarting.
    let out = session(
        &[
            "--interactive",
            "--estimate-only",
            "--threads",
            "1",
            "--address-type",
            "bech32",
        ],
        "1\nbQ\ncat\n1\n",
    );
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("must be lowercase"));
    assert!(stderr.contains("Preview    bc1qcat…"));
    assert!(stderr.contains("Measured:"));
    assert!(!stderr.contains("MATCH FOUND"));
}

#[test]
fn closed_input_and_missing_noninteractive_pattern_fail_without_keys() {
    for input in ["", "\n", "\nb\n5\n"] {
        let out = session(&["--interactive"], input);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).contains("input closed"));
    }
    let out = session(&[], "");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--interactive"));
    let out = session(&["--interactive"], "\nb\n99\n0\n");
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Choose a number from 0 to 5"));
}

#[test]
fn long_search_requires_affirmative_interactive_confirmation() {
    for (answer, succeeds) in [("\n", false), ("yes\n", true)] {
        let input = format!("\nb\n\n{answer}");
        let out = session(
            &["--interactive", "--threads", "1", "--max-eta-hours", "0"],
            &input,
        );
        assert_eq!(out.status.success(), succeeds);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("LONG SEARCH"));
        if succeeds {
            assert!(String::from_utf8_lossy(&out.stdout).contains("Private key (WIF compressed): "));
        } else {
            assert!(out.stdout.is_empty());
            assert!(stderr.contains("search cancelled"));
        }
    }
}

#[test]
fn settings_save_only_to_explicit_new_file() {
    let path = std::env::temp_dir().join(format!("vanitybtc-ui-{}.key", std::process::id()));
    let input = format!("\nb\n3\n2\n0\n1\n1\ny\n{}\n1\n", path.display());
    let out = session(&["--interactive"], &input);
    assert!(out.status.success());
    assert!(std::fs::read(&path).unwrap() == out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("threads must be between"));
    assert!(stderr.contains("CPU · 1 thread"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn queue_runs_in_selected_order_and_saves_every_verified_result() {
    let path = std::env::temp_dir().join(format!("vanitybtc-queue-{}.key", std::process::id()));
    // Duplicate Ethereum is ignored; preserve the explicitly chosen order.
    let out = session(
        &[
            "--interactive",
            "--threads",
            "1",
            "--output-file",
            path.to_str().unwrap(),
        ],
        "2,1,2,3\nb\n\n",
    );
    assert!(out.status.success());
    let saved = zeroize::Zeroizing::new(std::fs::read(&path).unwrap());
    std::fs::remove_file(&path).unwrap();
    assert!(saved.as_slice() == out.stdout);
    let text = zeroize::Zeroizing::new(String::from_utf8(out.stdout).unwrap());
    let records: Vec<_> = text.split("Chain: ").skip(1).collect();
    assert_eq!(records.len(), 3);
    let stderr = String::from_utf8_lossy(&out.stderr);
    for (index, (record, chain)) in records
        .iter()
        .zip(["ethereum", "bitcoin", "solana"])
        .enumerate()
    {
        assert_eq!(record.lines().next().unwrap(), chain);
        assert!(stderr.contains(&format!("[{}/3] {chain}", index + 1)));
        let address = field(record, "Address: ");
        if chain == "solana" {
            use ring::signature::{Ed25519KeyPair, KeyPair};
            let encoded = field(record, "Private key (base58, 64 bytes): ");
            let pair = zeroize::Zeroizing::new(bs58::decode(encoded).into_vec().unwrap());
            assert_eq!(pair.len(), 64);
            let key = Ed25519KeyPair::from_seed_and_public_key(&pair[..32], &pair[32..]).unwrap();
            assert_eq!(bs58::encode(key.public_key()).into_string(), address);
            assert!(address.starts_with('b'));
            assert!(!stderr.contains(encoded));
        } else {
            let raw = field(record, "Private key (hex): ");
            let mut secret: bitcoin::secp256k1::SecretKey = raw.parse().unwrap();
            let public = bitcoin::secp256k1::PublicKey::from_secret_key(
                &bitcoin::secp256k1::Secp256k1::new(),
                &secret,
            );
            secret.non_secure_erase();
            assert!(!stderr.contains(raw));
            if chain == "bitcoin" {
                let mut key =
                    bitcoin::PrivateKey::from_wif(field(record, "Private key (WIF compressed): "))
                        .unwrap();
                assert!(key.inner.display_secret().to_string() == raw);
                key.inner.non_secure_erase();
                assert_eq!(
                    bitcoin::Address::p2pkh(
                        bitcoin::PublicKey::new(public),
                        bitcoin::Network::Bitcoin
                    )
                    .to_string(),
                    address
                );
                assert!(address[1..].starts_with('b'));
            } else {
                use tiny_keccak::{Hasher, Keccak};
                let mut hash = Keccak::v256();
                hash.update(&public.serialize_uncompressed()[1..]);
                let mut digest = [0; 32];
                hash.finalize(&mut digest);
                let expected: String = digest[12..].iter().map(|b| format!("{b:02x}")).collect();
                assert_eq!(address[2..].to_ascii_lowercase(), expected);
                assert!(address[2..].starts_with('b'));
            }
        }
    }
    eprintln!("Queue check: Ethereum → Bitcoin → Solana, all private keys verified; combined output file matched stdout.");
}

#[test]
fn queue_validates_all_patterns_and_stops_after_declined_search() {
    let out = session(&["--interactive", "--threads", "1"], "1,2\ng\nb\n0\n");
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("ethereum cannot use this text"));
    assert!(!stderr.contains("[1/2]"));
    let out = session(
        &["--interactive", "--threads", "1", "--max-eta-hours", "0"],
        "2,1\nb\n\n\n",
    );
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("[1/2] ethereum"));
    assert!(!stderr.contains("[2/2] bitcoin"));
}

#[test]
fn queue_estimates_never_create_requested_file_or_emit_keys() {
    let path = std::env::temp_dir().join(format!(
        "vanitybtc-queue-estimate-{}.key",
        std::process::id()
    ));
    let out = session(
        &[
            "--interactive",
            "--threads",
            "1",
            "--estimate-only",
            "--output-file",
            path.to_str().unwrap(),
        ],
        "2,1\nb\n\n",
    );
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(!path.exists());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("[1/2] ethereum"));
    assert!(stderr.contains("[2/2] bitcoin"));
}

#[test]
fn minimal_text_prompt_has_length_estimates_and_no_hidden_default() {
    let out = session(&["--interactive", "--threads", "1"], "1\n\nb\n0\n");
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("VANITY GENERATOR"));
    assert!(stderr.contains("Characters     Rough ETA"));
    assert!(stderr.contains("Rough ETA  "));
    assert!(stderr.contains("Enter at least one character"));
    assert!(!stderr.contains("Enter = b"));
    assert!(!stderr.contains("What would you like"));
    assert!(!stderr.contains("V A N I T Y   L A B"));
}
