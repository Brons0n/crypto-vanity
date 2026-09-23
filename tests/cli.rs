use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vanitybtc"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn invalid_inputs_fail_before_benchmark() {
    for args in [
        vec!["--prefix", "0"],
        vec!["--prefix", "qQ", "--address-type", "bech32"],
        vec!["--prefix", "a", "--threads", "0"],
        vec!["--prefix", "a", "--max-eta-hours", "NaN"],
    ] {
        let out = run(&args);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&out.stderr).contains("Benchmarking"));
    }
}

#[test]
fn estimates_and_confirmation_never_emit_keys() {
    for args in [
        vec!["--prefix", "abc", "--threads", "1", "--estimate-only"],
        vec!["--prefix", "abc", "--threads", "1", "--max-eta-hours", "0"],
    ] {
        let out = run(&args);
        assert!(out.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("keys/sec"));
        assert!(!stderr.contains("Private key"));
        if args.contains(&"--estimate-only") {
            assert!(out.status.success());
        } else {
            assert!(!out.status.success());
            assert!(stderr.contains("--confirm"));
        }
    }
}

#[test]
fn actual_results_agree_with_independent_bitcoin_library() {
    check_results(false);
}

#[test]
#[ignore = "requires a compiled backend and GPU hardware"]
fn gpu_results_agree_with_independent_bitcoin_library() {
    check_results(true);
}

fn check_results(gpu: bool) {
    for kind in ["legacy", "bech32"] {
        let (prefix, suffix) = if kind == "legacy" {
            ("a", "7")
        } else {
            ("q", "p")
        };
        let mut args = vec![
            "--prefix",
            prefix,
            "--suffix",
            suffix,
            "--case-insensitive",
            "--threads",
            "2",
            "--address-type",
            kind,
            "--max-eta-hours",
            "0",
            "--confirm",
        ];
        if gpu {
            args.push("--gpu");
        }
        let out = run(&args);
        if gpu {
            assert!(
                String::from_utf8_lossy(&out.stderr).contains("startup verification passed"),
                "GPU was not selected"
            );
            assert!(!String::from_utf8_lossy(&out.stderr).contains("Falling back"));
            assert!(!String::from_utf8_lossy(&out.stderr).contains("Restarting"));
        }
        assert!(out.status.success());
        assert!(!String::from_utf8_lossy(&out.stderr).contains("Private key"));
        let text = String::from_utf8(out.stdout).unwrap();
        let value = |label: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(label))
                .unwrap()
        };
        let key = bitcoin::PrivateKey::from_wif(value("Private key (WIF compressed): ")).unwrap();
        assert!(key.compressed);
        let secp = bitcoin::secp256k1::Secp256k1::new();
        let public = key.public_key(&secp);
        assert_eq!(public.to_string(), value("Public key (hex compressed): "));
        // Avoid assertion diagnostics that could print secret material on failure.
        assert!(key.inner.display_secret().to_string() == value("Private key (hex): "));
        let expected = if kind == "legacy" {
            bitcoin::Address::p2pkh(public, bitcoin::Network::Bitcoin)
        } else {
            bitcoin::Address::p2wpkh(
                &bitcoin::CompressedPublicKey(public.inner),
                bitcoin::Network::Bitcoin,
            )
        };
        assert_eq!(expected.to_string(), value("Address: "));
        let fixed = if kind == "legacy" { 1 } else { 4 };
        assert!(value("Address: ")[fixed..]
            .to_ascii_lowercase()
            .starts_with(prefix));
        assert!(value("Address: ").ends_with(suffix));
    }
}

#[test]
fn explicit_file_is_private_and_never_overwritten() {
    let path = std::env::temp_dir().join(format!("vanitybtc-test-{}.key", std::process::id()));
    let path_str = path.to_str().unwrap();
    let out = run(&["--prefix", "", "--threads", "1", "--output-file", path_str]);
    assert!(out.status.success());
    assert!(std::fs::read(&path).unwrap() == out.stdout);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let refused = run(&["--prefix", "", "--output-file", path_str]);
    assert!(!refused.status.success());
    assert!(refused.stdout.is_empty());
    std::fs::remove_file(path).unwrap();
}

#[test]
#[cfg(not(any(
    feature = "gpu-opencl",
    all(feature = "gpu-metal", target_os = "macos")
)))]
fn gpu_flag_falls_back_in_cpu_only_build() {
    let out = run(&[
        "--prefix",
        "abc",
        "--gpu",
        "--threads",
        "1",
        "--estimate-only",
    ]);
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Falling back to CPU"));
}

#[test]
fn estimate_only_does_not_create_requested_file() {
    let path = std::env::temp_dir().join(format!("vanitybtc-estimate-{}.key", std::process::id()));
    let out = run(&[
        "--prefix",
        "a",
        "--threads",
        "1",
        "--estimate-only",
        "--output-file",
        path.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(!path.exists());
}

fn decode_hex(text: &str) -> zeroize::Zeroizing<Vec<u8>> {
    assert_eq!(text.len() % 2, 0);
    zeroize::Zeroizing::new(
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect(),
    )
}

#[test]
fn evm_cli_matches_independent_keccak_and_secp256k1() {
    use tiny_keccak::{Hasher, Keccak};
    for (chain, prefix, insensitive) in [
        ("ethereum", "a", true),
        ("ethereum", "A", false),
        ("bnb", "a", true),
        ("bsc", "A", false),
    ] {
        let mut args = vec![
            "--chain",
            chain,
            "--prefix",
            prefix,
            "--suffix",
            "7",
            "--threads",
            "2",
            "--max-eta-hours",
            "0",
            "--confirm",
        ];
        if insensitive {
            args.push("--case-insensitive");
        }
        let out = run(&args);
        assert!(out.status.success());
        let text = zeroize::Zeroizing::new(String::from_utf8(out.stdout).unwrap());
        let value = |label: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(label))
                .unwrap()
        };
        let address = value("Address: ");
        assert_eq!(
            value("Chain: "),
            if chain == "ethereum" {
                "ethereum"
            } else {
                "bnb"
            }
        );
        assert!(!text.contains("WIF"));
        let secret = decode_hex(value("Private key (hex): "));
        let mut key = bitcoin::secp256k1::SecretKey::from_slice(&secret).unwrap();
        let public = bitcoin::secp256k1::PublicKey::from_secret_key(
            &bitcoin::secp256k1::Secp256k1::new(),
            &key,
        )
        .serialize_uncompressed();
        key.non_secure_erase();
        assert_eq!(
            &public[..],
            &decode_hex(value("Public key (hex uncompressed): "))[..]
        );
        let mut digest = [0u8; 32];
        let mut hash = Keccak::v256();
        hash.update(&public[1..]);
        hash.finalize(&mut digest);
        let expected: String = digest[12..].iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(address[2..].to_ascii_lowercase(), expected);
        let mut case_hash = [0u8; 32];
        let mut hash = Keccak::v256();
        hash.update(expected.as_bytes());
        hash.finalize(&mut case_hash);
        for (i, c) in address[2..].bytes().enumerate() {
            if c.is_ascii_alphabetic() {
                let mask = if i % 2 == 0 { 0x80 } else { 8 };
                assert_eq!(c.is_ascii_uppercase(), case_hash[i / 2] & mask != 0);
            }
        }
        if insensitive {
            assert!(address[2..].to_ascii_lowercase().starts_with(prefix));
        } else {
            assert!(address[2..].starts_with(prefix));
        }
        assert!(address.ends_with('7'));
        // Boolean assertions avoid secret-bearing failure diagnostics.
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!stderr.contains(value("Private key (hex): ")));
    }
}

#[test]
fn solana_cli_seed_and_keypair_work_with_independent_ed25519() {
    use ring::signature::{Ed25519KeyPair, KeyPair};
    for (prefix, suffix, insensitive) in [("a", "", true), ("", "7", false)] {
        let mut args = vec![
            "--chain",
            "solana",
            "--prefix",
            prefix,
            "--suffix",
            suffix,
            "--threads",
            "2",
        ];
        if insensitive {
            args.push("--case-insensitive");
        }
        let out = run(&args);
        assert!(out.status.success());
        let text = zeroize::Zeroizing::new(String::from_utf8(out.stdout).unwrap());
        let value = |label: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(label))
                .unwrap()
        };
        assert_eq!(value("Chain: "), "solana");
        assert!(!text.contains("WIF"));
        let seed = decode_hex(value("Private seed (hex, 32 bytes): "));
        let pair = zeroize::Zeroizing::new(
            bs58::decode(value("Private key (base58, 64 bytes): "))
                .into_vec()
                .unwrap(),
        );
        let public = decode_hex(value("Public key (hex Ed25519): "));
        assert_eq!(seed.len(), 32);
        assert_eq!(pair.len(), 64);
        assert!(pair[..32] == seed[..]);
        assert!(pair[32..] == public[..]);
        let key = Ed25519KeyPair::from_seed_and_public_key(&seed, &public).unwrap();
        assert_eq!(key.public_key().as_ref(), &public[..]);
        assert_eq!(
            bs58::encode(key.public_key()).into_string(),
            value("Address: ")
        );
        let sig = key.sign(b"offline vanity key verification");
        let verifying =
            ed25519_dalek::VerifyingKey::from_bytes(public.as_slice().try_into().unwrap()).unwrap();
        verifying
            .verify_strict(
                b"offline vanity key verification",
                &ed25519_dalek::Signature::from_slice(sig.as_ref()).unwrap(),
            )
            .unwrap();
        if insensitive {
            assert!(value("Address: ").to_ascii_lowercase().starts_with(prefix));
        } else {
            assert!(value("Address: ").starts_with(prefix));
        }
        assert!(value("Address: ").ends_with(suffix));
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!stderr.contains(value("Private seed (hex, 32 bytes): ")));
        assert!(!stderr.contains(value("Private key (base58, 64 bytes): ")));
    }
}

#[test]
fn new_chain_validation_rejects_wrong_alphabets_and_bitcoin_flags() {
    for args in [
        vec!["--chain", "ethereum", "--prefix", "g"],
        vec!["--chain", "solana", "--prefix", "0"],
        vec![
            "--chain",
            "ethereum",
            "--prefix",
            "a",
            "--address-type",
            "legacy",
        ],
        vec![
            "--chain",
            "solana",
            "--prefix",
            "a",
            "--address-type",
            "bech32",
        ],
        vec!["--chain", "unknown", "--prefix", "a"],
    ] {
        let out = run(&args);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&out.stderr).contains("Benchmarking"));
    }
}

#[test]
fn base58_chains_match_independent_hashes_keys_and_network_versions() {
    use bitcoin::hashes::{hash160, Hash};
    use tiny_keccak::{Hasher, Keccak};
    for (chain, canonical, prefix, version, wif_version) in [
        ("ripple", "xrp", "U", 0, None),
        ("trx", "tron", "N", 0x41, None),
        ("doge", "dogecoin", "P", 0x1e, Some(0x9e)),
        ("ltc", "litecoin", "W", 0x30, Some(0xb0)),
    ] {
        let out = run(&[
            "--chain",
            chain,
            "--prefix",
            prefix,
            "--suffix",
            "7",
            "--threads",
            "2",
        ]);
        assert!(out.status.success());
        let text = zeroize::Zeroizing::new(String::from_utf8(out.stdout).unwrap());
        let value = |label: &str| text.lines().find_map(|l| l.strip_prefix(label)).unwrap();
        assert_eq!(value("Chain: "), canonical);
        let secret = decode_hex(value("Private key (hex): "));
        let mut key = bitcoin::secp256k1::SecretKey::from_slice(&secret).unwrap();
        let public = bitcoin::secp256k1::PublicKey::from_secret_key(
            &bitcoin::secp256k1::Secp256k1::new(),
            &key,
        );
        key.non_secure_erase();
        let address = value("Address: ");
        // Translate the independently specified XRP alphabet to Bitcoin's alphabet,
        // then use the separate bitcoin crate's Base58Check decoder (including checksum).
        let encoded = if canonical == "xrp" {
            let ripple = b"rpshnaf39wBUDNEGHJKLM4PQRST7VWXYZ2bcdeCg65jkm8oFqi1tuvAxyz";
            let bitcoin = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
            address
                .bytes()
                .map(|c| bitcoin[ripple.iter().position(|&a| a == c).unwrap()] as char)
                .collect::<String>()
        } else {
            address.into()
        };
        let decoded = bitcoin::base58::decode_check(&encoded).unwrap();
        assert_eq!(decoded.len(), 21);
        assert_eq!(decoded[0], version);
        if canonical == "tron" {
            let uncompressed = public.serialize_uncompressed();
            let mut digest = [0u8; 32];
            let mut hash = Keccak::v256();
            hash.update(&uncompressed[1..]);
            hash.finalize(&mut digest);
            assert_eq!(&decoded[1..], &digest[12..]);
            assert_eq!(
                &decode_hex(value("Public key (hex uncompressed): "))[..],
                &uncompressed
            );
        } else {
            assert_eq!(
                &decoded[1..],
                hash160::Hash::hash(&public.serialize()).as_byte_array()
            );
            assert_eq!(
                &decode_hex(value("Public key (hex compressed): "))[..],
                &public.serialize()
            );
        }
        if let Some(version) = wif_version {
            let wif = zeroize::Zeroizing::new(
                bitcoin::base58::decode_check(value("Private key (WIF compressed): ")).unwrap(),
            );
            assert!(wif.len() == 34 && wif[0] == version && wif[33] == 1);
            assert!(wif[1..33] == secret[..]);
        } else {
            assert!(!text.contains("WIF"));
        }
        if canonical == "xrp" {
            let xrpl = decode_hex(value("Private key (XRPL hex): "));
            assert!(xrpl.len() == 33 && xrpl[0] == 0 && xrpl[1..] == secret[..]);
        }
        assert!(address[1..].starts_with(prefix));
        assert!(address.ends_with('7'));
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!stderr.contains(value("Private key (hex): ")));
        assert!(!stderr.contains("Private key"));
    }
}

#[test]
fn added_chains_reject_invalid_inputs_before_benchmark() {
    for chain in ["bnb", "xrp", "tron", "doge", "litecoin"] {
        for tail in [
            vec!["--prefix", "!"],
            vec!["--prefix", "", "--address-type", "legacy"],
        ] {
            let mut args = vec!["--chain", chain];
            args.extend(tail);
            let out = run(&args);
            assert!(!out.status.success());
            assert!(out.stdout.is_empty());
            assert!(!String::from_utf8_lossy(&out.stderr).contains("Benchmarking"));
        }
    }
    for chain in ["doge", "litecoin", "tron"] {
        let out = run(&["--chain", chain, "--prefix", "zz", "--confirm"]);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr).contains("address range"));
        assert!(!String::from_utf8_lossy(&out.stderr).contains("Benchmarking"));
    }
}

#[test]
fn new_chains_gpu_fallback_estimates_without_creating_files() {
    for chain in ["ethereum", "solana", "bnb", "xrp", "tron", "doge", "ltc"] {
        let path = std::env::temp_dir().join(format!(
            "vanitybtc-{chain}-estimate-{}.key",
            std::process::id()
        ));
        let out = run(&[
            "--chain",
            chain,
            "--prefix",
            "",
            "--suffix",
            "7",
            "--gpu",
            "--threads",
            "1",
            "--estimate-only",
            "--output-file",
            path.to_str().unwrap(),
        ]);
        assert!(out.status.success());
        assert!(out.stdout.is_empty());
        assert!(!path.exists());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("GPU backends currently support Bitcoin only"));
        assert!(stderr.contains("Falling back to CPU"));
        assert!(stderr.contains("Measured:"));
    }
}
