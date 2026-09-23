# crypto-vanity

Free and open-source under the [MIT License](LICENSE). You can use, modify, and redistribute it, including commercially, under that license.

An offline Rust CLI, `vanitybtc`, for finding **Bitcoin, Ethereum, Solana, BNB Smart Chain, XRP, TRON, Dogecoin, and Litecoin** vanity addresses. It supports **macOS on Apple Silicon and Intel**, Windows, and Linux from one Rust codebase. The executable name stays `vanitybtc` for compatibility.

| Chain | Select | Address | Private material printed on a match |
| --- | --- | --- | --- |
| Bitcoin (default) | `--chain bitcoin` | Compressed-key P2PKH (`1…`) or P2WPKH (`bc1q…`) | Compressed WIF and 32-byte scalar hex |
| Ethereum | `--chain ethereum` | EIP-55 checksummed `0x…` externally owned account | 32-byte scalar hex |
| Solana | `--chain solana` | Base58 Ed25519 public key | 32-byte private seed hex and Base58 of the 64-byte seed + public key |
| BNB Smart Chain | `--chain bnb` | EIP-55 checksummed `0x…` externally owned account | 32-byte scalar hex |
| XRP Ledger | `--chain xrp` | Classic `r…`, secp256k1 | 32-byte scalar hex and XRPL `00`-prefixed private hex |
| TRON | `--chain tron` | Base58Check `T…` | 32-byte scalar hex |
| Dogecoin | `--chain doge` | Compressed-key mainnet P2PKH `D…` | Dogecoin compressed WIF and 32-byte scalar hex |
| Litecoin | `--chain litecoin` | Compressed-key mainnet P2PKH `L…` | Litecoin compressed WIF and 32-byte scalar hex |

All chains support prefix/suffix matching, case-insensitive matching, multithreaded CPU search, machine-specific estimates, progress, cancellation, and explicitly requested output files. **GPU acceleration currently supports Bitcoin only**; requesting `--gpu` for any other chain prints a warning and uses the CPU.

## Guided interface — no long commands

After building once, open **Vanity Generator**, the built-in terminal interface:

- **macOS:** double-click `Start Vanity.command` in the project folder. The window stays open after a match so you can read the result.
- **Windows:** double-click `start-vanity.bat`.
- **Linux:** run `./start-vanity.sh` in a terminal.
- **macOS terminal:** run `./start-vanity.sh` or `./target/release/vanitybtc` with no arguments.

Windows, Linux, and macOS use the same Rust interface, layout, monochrome styling, network queue, and exact-case default. The Windows launcher selects UTF-8 while running and restores the original console code page afterward. Font rendering comes from your terminal; the app does not change your system theme.

After the [GitHub build workflow](https://github.com/Brons0n/crypto-vanity/actions/workflows/ci.yml) succeeds, each run provides a native download under **Actions → Build and test → the run → Artifacts**: Windows x64 (`.zip`), Linux x64 (`.tar.gz`), macOS Apple Silicon (`.tar.gz`), and macOS Intel (`.tar.gz`). Extract both the downloaded artifact and the archive inside it. The binary and launcher sit together; Rust is not needed to run these builds. On Linux/macOS, extract the `.tar.gz` to preserve executable permissions. On Linux run `./start-vanity.sh` from the extracted folder; on Windows/macOS use the launchers above. CPU-only downloads always work without a GPU toolchain. The Linux download is built on Ubuntu 24.04; older Linux distributions may require building from source.

These downloads are created only after that platform's tests pass and its packaged launcher successfully displays the shared interface. They are CI artifacts, not automatically published GitHub Releases. The [GitHub workflow](.github/workflows/ci.yml) uses native [GitHub-hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners) and [artifact uploads](https://github.com/actions/upload-artifact).

The black-and-white interface starts with **network selection**. Press Enter for Bitcoin, or enter several numbers separated by commas, such as `1,2,3` for Bitcoin, Ethereum, and Solana. The order you enter is the search order; duplicates are ignored.

Next, a short CPU speed check shows a **rough ETA table for 1–8 characters** on the first selected network. Type your text at the plain **Text** prompt, then press Enter to choose **Create my address**. There is no prefilled pattern or hidden `b` default. The review shows the selected networks, preview, and an estimate for your actual pattern. Longer text is usually much slower. The table uses alphabet-based ranges and measured CPU throughput; specific characters, encoding constraints, and GPU performance can change the real time. The selected search engine is benchmarked again before each search.

For a quick test: press Enter to select Bitcoin, type `b`, then press Enter to start. With multiple networks, the same pattern is checked on every selected network before starting. The tool finds and prints one address/private-key result, then advances to the next selected network. It stops the queue if you cancel, decline a long search, or a search fails. Previous completed results remain available. No network requests are made.

**Change network** edits the selection; **Change text** edits your pattern; **More options → Text position and letter case** supports suffixes, both ends, and exact letter case. **More options → Speed and saving to a file** controls CPU threads, Bitcoin GPU acceleration, and optional file output. If you explicitly select a new output file, all completed results are written to that one file in order, without overwriting existing files. No file is saved by default. `0` exits a menu. Empty text and invalid patterns can be corrected in place.

Bitcoin SegWit excludes `b` from its searchable alphabet; use `q` for a single-network test. Ethereum and BNB require hexadecimal text. A multi-network pattern must be valid for all selected formats. Matching is case-sensitive by default: `abc` matches only `abc`, and `AbC` matches only `AbC` in the requested part of the address. Ignore-case matching remains optional through More options or `--case-insensitive`; bech32 patterns always remain lowercase. Long searches require an explicit yes after measuring speed; Enter declines. The monochrome theme uses only bold/dim text and respects `NO_COLOR`.

`--interactive` (or `-i`) explicitly opens the same interface and can be combined with existing options. With no arguments in a noninteractive process, the tool exits with guidance instead of waiting for input. The original command-line options below remain available; their exact-case default is unchanged.

## Build the CPU version

Install Rust 1.85 or newer and a C compiler. All platforms use the same source code:

- **Windows:** install Rust with the MSVC toolchain and Visual Studio Build Tools with “Desktop development with C++” and the Windows SDK. Run the commands below in a developer terminal. The executable is `target\release\vanitybtc.exe`.
- **macOS (Apple Silicon or Intel):** install the Command Line Tools with `xcode-select --install`, then Rust for your native architecture. The executable is `target/release/vanitybtc`.
- **Linux:** install Rust and your distribution's C build tools (for example, `build-essential` on Debian/Ubuntu). The executable is `target/release/vanitybtc`.

```sh
cargo fetch --locked
cargo test --locked --offline
cargo build --release --locked --offline
```

Dependency fetching needs internet access on the build machine. The compiled program has no networking functionality and requires no network access. For an air-gapped build, prepare the dependencies beforehand with `cargo vendor --locked` and transfer the source, vendor directory, and Cargo source-replacement configuration. No dependency is intentionally configured to phone home; the dependency tree contains no HTTP client or telemetry SDK. This is a source/dependency review, not a formal audit of every compiler or driver.

## Usage

```sh
# Ethereum: match after 0x; preserve the correct EIP-55 casing in the output
./target/release/vanitybtc --chain ethereum --prefix dead --case-insensitive
./target/release/vanitybtc --chain ethereum --prefix cafe --suffix 123 --case-insensitive --estimate-only

# Solana: match from the very first address character (there is no fixed prefix)
./target/release/vanitybtc --chain solana --prefix Cat
./target/release/vanitybtc --chain solana --prefix '' --suffix abc --threads 4

# BNB Smart Chain: same hexadecimal pattern rules as Ethereum
./target/release/vanitybtc --chain bnb --prefix cafe --case-insensitive

# Prefixes start AFTER the fixed r / T / D / L character
./target/release/vanitybtc --chain xrp --prefix Cat
./target/release/vanitybtc --chain tron --prefix Cat
./target/release/vanitybtc --chain doge --prefix Cat
./target/release/vanitybtc --chain litecoin --prefix Wow

# Suffix-only works on every chain
./target/release/vanitybtc --chain doge --prefix '' --suffix wow --case-insensitive
```

`--chain` accepts `bitcoin`, `ethereum`, `solana`, `bnb`, `xrp`, `tron`, `dogecoin`, and `litecoin` (aliases: `btc`, `eth`, `sol`, `bsc`, `ripple`, `trx`, `doge`, `ltc`, respectively). Omitting it preserves the existing Bitcoin behavior. `--address-type legacy|bech32` applies only to Bitcoin; using it with another chain is rejected.

Ethereum and BNB Smart Chain patterns contain hexadecimal digits `0-9`, `a-f`, or `A-F`, **without `0x` in the prefix**. The default matches the exact EIP-55 case; `--case-insensitive` matches the underlying hex value and still prints the checksummed address. For the rough estimate, a digit costs a factor of 16, an exact-case letter approximately 32, and a case-insensitive letter 16. Solana uses the same Base58 alphabet as Bitcoin legacy addresses but no checksum or fixed leading character. Case-insensitive matching never changes the case of a generated Base58 address, since case is part of its value. XRP uses the Ripple Base58 alphabet. Prefix matching skips exactly the initial `r` for XRP, `T` for TRON, `D` for Dogecoin, or `L` for Litecoin. The network version also restricts subsequent characters for TRON, Dogecoin, and Litecoin; impossible network-range patterns are rejected before benchmarking. For example, a Dogecoin prefix of `oge` would request `Doge…`, which is outside its address range. A suffix-only search avoids this restriction.

Existing Bitcoin examples:

```sh
./target/release/vanitybtc --prefix Cat
./target/release/vanitybtc --prefix cat --case-insensitive --threads 4
./target/release/vanitybtc --prefix cash --address-type bech32
./target/release/vanitybtc --prefix '' --suffix abc
./target/release/vanitybtc --prefix abc --suffix 7 --estimate-only
./target/release/vanitybtc --prefix LongPattern --max-eta-hours 24 --confirm
./target/release/vanitybtc --prefix Cat --output-file ./result.key
```

In Windows Command Prompt use `--prefix ""` for suffix-only searches. For Bitcoin, the prefix starts immediately after `1` or `bc1q`. Both prefix and suffix must match. Empty patterns match any key. Base58 excludes `0`, `O`, `I`, and `l`; case-insensitive searches accept a character if at least one of its case variants is valid. Bech32 patterns must be lowercase, including when `--case-insensitive` is set; uppercase/mixed-case inputs receive a clear error. Bech32 excludes `1`, `b`, `i`, and `o` from its searchable portion.

`--threads` defaults to the number of logical CPUs. A 1.5-second benchmark measures the selected chain's complete key-generation/point-addition, address-encoding, and pattern-checking pipeline. Benchmark attempts are discarded and excluded from the final search totals. Search workers then start from fresh, independently sampled OS-random keys.

The preflight prints an alphabet-based search space, the requested half-space estimate, and the statistical mean. For independent random attempts with success probability `1/N`, the mean is **N**, the median is about **0.693N**, and **N/2 is not the expected number of attempts**. The 24-hour guard uses the mean; configure it with `--max-eta-hours`. `--estimate-only` runs only the benchmark, emits no keys, and never creates an output file. `--confirm` bypasses the time guard, not input validation.

These are rough estimates: Base58 address lengths and initial characters are nonuniform; long patterns can constrain checksum characters and may be impossible. Case variants and overlapping constraints are counted once. Random search has no deadline, and its conditional expected remaining time does not decrease as unsuccessful attempts accumulate. Progress goes to stderr, updates at most four times per second on a terminal, and shows elapsed time, attempts, recent throughput, and estimated remaining mean. Redirected stderr omits live updates. Ctrl-C stops workers and emits no unfinished key.

## Implementation and verification

For every supported chain except Solana, each CPU thread obtains a private scalar from `getrandom`, backed by the operating system's cryptographic random source. It derives the initial point with `secp256k1`, then advances using `P := P + G` and `k := k + 1`, rather than doing scalar multiplication for every candidate. The invalid scalar at the group order is skipped by reseeding. Independent random starts make overlap negligibly likely; they are not a mathematical partition of the keyspace. A new scalar multiplication verifies every winning key/point/address before output.

Ethereum uses **Keccak-256**, not the standardized SHA3-256 variant. It hashes the 64-byte uncompressed public point without the SEC1 `04` prefix, takes the final 20 bytes, and applies [EIP-55](https://eips.ethereum.org/EIPS/eip-55). The printed Ethereum and BNB public key is the full 65-byte uncompressed SEC1 representation, including `04`. `--chain bnb` targets [BNB Smart Chain's Ethereum-compatible accounts](https://docs.bnbchain.org/bnb-smart-chain/developers/wallet-configuration/), with the same EIP-55 encoding. It does not generate legacy Beacon Chain `bnb1…` addresses.

[XRP classic addresses](https://xrpl.org/docs/references/protocol/data-types/base58-encodings) use version byte `0x00`, HASH160 of the compressed secp256k1 public key, a double-SHA-256 checksum, and the Ripple Base58 alphabet. The printed XRP public key is compressed (33 bytes). The output includes both the raw 32-byte private scalar and the XRPL keypair API's `00`-prefixed private hex representation. **These are direct private keys, not XRP family seeds** (`s…`). Recovery/signing software must support a direct secp256k1 keypair; software that accepts only family seeds cannot use this output. No X-address or destination tag is generated.

[TRON addresses](https://developers.tron.network/docs/account) use the same final 20 Keccak bytes as Ethereum, prepended with `0x41`, then Bitcoin-alphabet Base58Check encoding. The printed TRON public key is the full 65-byte uncompressed SEC1 point. Dogecoin and Litecoin use HASH160 of the compressed key with their mainnet version bytes: [Dogecoin](https://github.com/dogecoin/dogecoin/blob/master/src/chainparams.cpp) address `0x1e` / WIF `0x9e`; [Litecoin](https://github.com/litecoin-project/litecoin/blob/master/src/chainparams.cpp) address `0x30` / WIF `0xb0`. Litecoin support here is legacy P2PKH. Creating any of these keypairs is entirely local and does not activate or register an on-chain account.

Solana uses `ed25519-dalek` and a fresh OS-random 32-byte seed for each attempt. Standard Ed25519 hashes and clamps that seed before scalar multiplication, so Bitcoin's `k + 1 / P + G` optimization cannot recover the corresponding next seed. Solana therefore performs full seed-based key derivation in parallel across CPU threads. This produces conventional signing keys, not program-derived addresses (PDAs), and is generally slower. A fresh derivation verifies each winning seed/public-key pair before output. The 64-byte Base58 private key is **seed || public key**, not a 64-byte scalar or an expanded Ed25519 secret. The hex seed is binary key material, not a mnemonic/seed phrase. Optional output files remain human-readable text reports, not wallet JSON files.

Unit tests include these publicly documented vectors:

- [Bitcoin Wiki's compressed P2PKH derivation](https://en.bitcoin.it/wiki/Technical_background_of_Bitcoin_addresses): scalar `18e14a7b6a307f426a94f8114701e7c8e774e7f9a47e2c2035db29a206321725` → `1PMycacnJaSqwwJqjawXBErnLsZ7RkXUAs`.
- [BIP173 examples](https://github.com/bitcoin/bips/blob/master/bip-0173.mediawiki#examples): scalar 1 / generator point → `bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4`.

- [Go Ethereum's crypto tests](https://github.com/ethereum/go-ethereum/blob/master/crypto/crypto_test.go): private scalar `289c2857d4598e37fb9647507e47a309d6133539bf21a8b9cb6df88fd5232032` → Ethereum address `0x970e8128ab834e8eac17ab8e3812f010678cf791` (shown without checksum casing), plus all eight [EIP-55 checksum examples](https://eips.ethereum.org/EIPS/eip-55).
- [RFC 8032, section 7.1](https://www.rfc-editor.org/rfc/rfc8032#section-7.1): Ed25519 test 1 seed, public key, and empty-message signature; the public key encodes as Solana address `FVen3X669xLzsi6N2V91DoiyzHzg1uAgqiT8jZ9nS96Z`.

Additional vectors cover [XRPL's published secp256k1 keypair](https://github.com/XRPLF/xrpl.js/blob/main/packages/ripple-keypairs/test/fixtures/api.json), [java-tron's private/public/address fixture](https://github.com/tronprotocol/java-tron/blob/develop/framework/src/test/java/org/tron/common/crypto/ECKeyTest.java), and two compressed WIF/address pairs each from [Dogecoin Core](https://github.com/dogecoin/dogecoin/blob/master/src/test/key_tests.cpp) and [Litecoin Core](https://github.com/litecoin-project/litecoin/blob/master/src/test/key_tests.cpp). BNB uses the same known EVM derivation and EIP-55 vectors. Actual CLI searches for all five added chains are checked using independent secp256k1, Keccak, HASH160, and Base58Check implementations, including network-specific WIF and XRP private-key formatting.

The public test scalars and seeds are deliberately insecure; never send funds to them. Tests also compare incremental points to independent scalar multiplications, exercise the group-order boundary, and cross-check actual CLI results and WIF with the separate `bitcoin` crate. Ethereum CLI tests independently check the Keccak hash and checksum with `tiny-keccak` and a second secp256k1 version. Solana CLI tests reconstruct the printed seed/keypair using `ring`, verify the address, and cross-check Ed25519 signatures. These verification libraries are test-only. CLI tests capture results instead of logging generated private keys.

## Security

**Best practice is to run this on an offline/air-gapped machine.** Before sending funds, verify the address on a block explorer using only the public address on a separate online device. A block explorer can check address syntax/history, but it cannot prove that your private key controls the address. Independently verify the key/address relationship with trusted offline software. Never enter a private key into a website.

Private keys appear only in the final stdout result and, if explicitly requested, `--output-file`. No result files are created by default. Avoid shell recording, shared terminals, cloud-synced folders, and exposing stdout. The optional file is created exclusively (existing files and symlinks are not overwritten); on Unix it uses mode 0600. On Windows it inherits directory ACLs, so use a private directory. A cancelled or failed search can leave the explicitly requested file empty. Files are plaintext.

Owned private key arrays and formatted output buffers use `zeroize`. Temporary `secp256k1` secret values use the crate's best-effort erase operation. Solana seed/keypair buffers use `Zeroizing`, and `ed25519-dalek` is built with its zeroize-on-drop support. Compiler copies, library scratch space, OS buffers, swap, crash dumps, terminal scrollback, and driver memory cannot be guaranteed erased. There is no claim of formal security auditing. Keep backups of the resulting private key securely; losing it loses access to funds.

This tool has no wallet import/export workflow, seed phrases, network requests, or transaction/broadcast functionality.


## Optional GPU builds

GPU support is for Bitcoin legacy and bech32 addresses. All other supported chains use the multithreaded CPU path, including when `--gpu` is requested. CPU is always the default and is built and tested without either GPU feature. Use `--gpu` to opt in at runtime. Build one of:

```sh
# Windows / Linux: NVIDIA or AMD with an installed OpenCL 1.2+ driver/ICD
cargo build --release --locked --offline --features gpu-opencl

# macOS: Apple Silicon or a Metal-capable AMD Mac
cargo build --release --locked --offline --features gpu-metal

./target/release/vanitybtc --prefix cat --gpu --case-insensitive
```

Fetch dependencies on the build machine before the offline build. OpenCL uses dynamic runtime loading: no OpenCL SDK, headers, or link-time GPU libraries are required. The installed vendor driver must provide `OpenCL.dll` on Windows or `libOpenCL.so.1` and a usable vendor ICD on Linux. Metal uses the system framework and compiles the embedded Metal Shading Language source at runtime; no separate shader compilation step is required. Its Rust implementation and dependencies are compiled only on macOS. Enabling `gpu-metal` elsewhere leaves a working CPU binary. When both features are enabled on macOS, `--gpu` selects Metal. OpenCL on macOS is available for development/testing, but Metal is the intended Mac backend.

Both backends process 256 independent lanes × 16 successive points per dispatch, with point addition, SHA-256, RIPEMD-160, address encoding, and prefix/suffix checking performed on-device. A shared arithmetic source is compiled as OpenCL C or Metal Shading Language. Jacobian point addition and batch inversion amortize field inversions across 16 points. The GPU receives public points only; private starting scalars remain in zeroizing CPU buffers. Persistent device state avoids copying points back after each batch. OpenCL reads a four-byte completion flag each batch and retrieves match offsets only on a hit. Metal uses CPU/GPU shared storage directly, with completion synchronization and no per-batch buffer copies on Apple Silicon.

Every GPU initialization runs deterministic address self-tests for both encodings and checks advancement across dispatches. Every winning result is independently rederived and matched on the CPU before output. Missing drivers, unsupported hardware, compilation failures, failed self-tests, and reported dispatch errors print a warning and fall back to CPU. A search-time GPU error restarts from fresh CPU keys and counters, remeasures CPU throughput, and reapplies the time guard. A hung or crashed vendor driver cannot be reliably recovered by application-level error handling.

GPU throughput depends heavily on the device and driver; these portable kernels are not vendor-tuned and **GPU mode is not guaranteed faster than CPU mode**. Compare `--estimate-only` with and without `--gpu` on your machine. Initialization, shader compilation, and self-tests take additional time before the roughly 1.5-second benchmark. GPU cancellation is checked between batches. `--threads` controls the CPU search/fallback, not the GPU lane count. GPU search totals include the entire winning batch.

## Verification commands

```sh
cargo test --locked --offline --no-default-features
cargo test --locked --offline --all-features
cargo clippy --locked --offline --all-targets --all-features -- -D warnings
# Optional independent kernel arithmetic check (Python 3 + clang; macOS/Linux)
python3 tools/verify_kernel_math.py

# Explicit hardware tests (fail if no working GPU is available)
cargo test --locked --offline --features gpu-opencl -- --ignored
cargo test --locked --offline --features gpu-metal -- --ignored
```

Hardware tests are ignored by default so normal builds/tests work without a GPU. The checked-in CI workflow builds/tests on Linux, Windows, Apple Silicon macOS, and Intel macOS, and checks optional-feature compilation without GPU SDKs. CI hardware availability is not assumed. See [VALIDATION.md](VALIDATION.md) for the actual local verification record and remaining platform coverage.
