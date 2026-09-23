# Verification record

Verified locally on an Apple M5 Mac using Rust 1.95.0. The CPU build and published address-vector tests passed before GPU implementation began. OpenCL was implemented and hardware-tested first; Metal was implemented and tested afterward.

The current CPU suite covers Bitcoin, Ethereum, Solana, BNB Smart Chain, XRP, TRON, Dogecoin, and Litecoin. All eight chains support CPU search; Bitcoin retains both optional GPU backends. The five latest additions require no new dependencies.

## Passed

The current interface is **Vanity Generator**, with a monochrome theme, a plain required Text input, and measured CPU ETA ranges by character count. Network selection appears first and supports an ordered queue of networks. Exact letter case is now the default, including queued searches; UI key-verification tests require the requested lowercase `b` exactly, while explicitly selected ignore-case matching remains available.

- Ten interactive integration tests cover all eight network choices, text validation, measured ETA previews, EOF/cancellation, estimate-only behavior, explicit long-search approval, and optional file saving with Unix mode 0600.
- A real Ethereum → Bitcoin → Solana queue produces three sequential results. Every private key is independently checked against its address using the separate Bitcoin, Keccak, and Ed25519 test libraries; duplicate network selections are ignored, and one explicitly requested output file contains exactly the combined stdout results.
- Queue tests verify all patterns before searching, stop after declined confirmation without starting subsequent networks, and perform multi-network estimate-only runs without creating a file or emitting keys.

- Release-mode interactive searches generated an exact lowercase `1b…` Bitcoin address and a `bc1qq…` SegWit address. Both returned WIF and raw-hex private keys plus the public key; the separate `bitcoin` library reconstructed the keys and independently reproduced both addresses. Test logs contain public addresses only.
- A real terminal session verified automatic UI startup with no arguments. The executable macOS launcher was exercised with a clean exit, and Ctrl-C at the post-benchmark long-search prompt was confirmed to exit promptly without key output.

- CPU-only unit/CLI suite on native `aarch64-apple-darwin`: 43 tests, with the GPU-only CLI test intentionally ignored.
- CPU-only suite compiled for `x86_64-apple-darwin` and executed under Rosetta: 43 tests. This verifies the Intel binary on macOS, not physical Intel hardware.
- All-feature unit/CLI suite: 42 ordinary tests; GPU hardware tests are opt-in.
- Published compressed P2PKH and BIP173 P2WPKH vectors, compressed WIF for scalar 1, and private/public/address agreement against the separate `bitcoin` library.
- Ethereum private-key/address derivation against the Go Ethereum vector; all eight EIP-55 checksum vectors; actual exact-case and case-insensitive CLI searches independently checked with `tiny-keccak` and the Bitcoin test dependency's separate secp256k1 version.
- Solana seed/public-key/signature derivation against RFC 8032 test 1, including its Base58 address; actual prefix and suffix searches whose printed 32-byte seed and 64-byte keypair reconstruct correctly in `ring`, with cross-library signature verification.
- BNB EIP-55 derivation and exact/case-insensitive searches; XRPL's published secp256k1 private/public/classic-address fixture; java-tron's published placeholder scalar/public/address fixture; two mainnet compressed WIF/address fixtures each from Dogecoin Core and Litecoin Core.
- Actual BNB, XRP, TRON, Dogecoin, and Litecoin CLI results independently rederived using a second secp256k1 version, `tiny-keccak`, and the `bitcoin` crate's HASH160 and Base58Check implementation. XRP uses its separate alphabet and the `00` private-key type prefix; Dogecoin/Litecoin WIF network bytes and compressed markers are checked.
- Rejection of impossible Dogecoin/Litecoin/TRON address ranges before benchmarking, including case variants; 64 deterministic scalars across all four added Base58 chains checked against full prefix/suffix patterns. New-chain aliases, invalid alphabets, Bitcoin-only flag rejection, GPU fallback, and estimate-only no-output/no-file behavior passed.
- Ethereum hex/case validation and per-character estimates, Solana Base58 matching from the first character, invalid Bitcoin-only flags on other chains, CPU fallback for `--gpu` on new chains, and estimate-only behavior without secret output or files.
- Bitcoin OpenCL and Metal hardware regressions passed separately in the prior Ethereum/Solana implementation pass. The five-chain and interactive-UI passes reran CPU and all-feature ordinary tests; GPU kernels were unchanged and hardware tests were not rerun for this pass.
- Hundreds of incremental CPU public points checked against fresh scalar multiplication; reseeding at the scalar-order boundary; multithreaded search and cancellation.
- Invalid alphabets, uppercase/mixed-case bech32 rejection, incompatible overlapping patterns, full-address suffixes, checksum validation for fully specified bech32 patterns, and case-insensitive matching.
- Estimate-only does not emit keys or create requested files; the ETA confirmation guard and override work; a missing/failed GPU falls back to CPU.
- Explicit output files match stdout, use Unix mode 0600, and cannot overwrite an existing file.
- Metal hardware tests on Apple M5, including exact address matching across consecutive batches, diverse lanes, boundary scalars, case-insensitive prefix/suffix checks, and full CLI key/address verification.
- OpenCL hardware tests using this Mac's OpenCL driver, including the same kernel/differential checks and full CLI verification.
- Independent check of the actual shared GPU arithmetic against Python big integers: 2,138 field pairs, 99 inversions, 128 point additions, and both generator-point address vectors (`python3 tools/verify_kernel_math.py`).
- CPU and all-feature release builds; Intel-target all-feature compilation; formatting and all-target/all-feature Clippy with warnings denied.
- Manual terminal check: progress overwrites in place at roughly four updates per second; Ctrl-C stops the search cleanly without printing an unfinished private key.

CLI tests capture secret-bearing stdout rather than displaying it in test logs. Source and runtime dependency-tree inspection found no application network calls or telemetry clients. The UI uses `anstream`, already present through Clap, for cross-platform terminal colors; it introduces no network dependency. Builds/tests after dependency preparation were run with Cargo's offline mode. GPU driver internals and OS services have not been formally audited.

## Short performance sample

Original Bitcoin release build, lowercase bech32 prefix `cat`, approximately 1.5-second benchmark:

| Engine | Measured throughput |
| --- | ---: |
| CPU, 10 logical threads | 1,688,427 keys/sec |
| Metal, Apple M5 | 2,566,590 keys/sec |

These are single local samples, not portable guarantees. Different address encodings, hardware, thermal state, and system load change throughput. Each invocation measures its own rate.

## Remaining platform coverage

All platforms compile the same UI module. The Linux and macOS launchers now share `start-vanity.sh`; Windows uses the same binary interface with UTF-8 console setup. Local macOS packaging and startup checks passed, including the centered title, absent subtitle, and monochrome UI. `.gitattributes` keeps Unix launchers on LF and Windows batch files on CRLF when checked out.

The GitHub workflow now prepares native CPU download archives for Windows x64, Linux x64, macOS Apple Silicon, and macOS Intel after the tests pass. Packaging runs the native launcher and checks the shared UI before uploading an artifact. Only the executable, launchers, README, and license are packaged. No native Windows/Linux package has been built or tested locally; that coverage remains pending the first GitHub run.

Windows and Linux executables were not run locally. No NVIDIA GPU, AMD GPU, physical Intel Mac, or AMD Mac was available. The checked-in GitHub Actions workflow covers Windows, Linux, Apple Silicon macOS, Intel macOS, and a minimum-Rust CPU check; that remote workflow has not been executed from this workspace. Generic CI jobs do not claim GPU hardware coverage.

The OpenCL and Metal backends reject initialization if their mandatory on-device self-tests fail, and every returned match is independently rederived on the CPU before output. These checks are useful safeguards, not a formal cryptographic or driver audit.
