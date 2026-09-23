use clap::ValueEnum;
use ripemd::Ripemd160;
use secp256k1::PublicKey;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum AddressType {
    Legacy,
    Bech32,
    Ethereum,
    Solana,
    Bnb,
    Xrp,
    Tron,
    Dogecoin,
    Litecoin,
}

impl AddressType {
    pub fn is_bitcoin(self) -> bool {
        matches!(self, Self::Legacy | Self::Bech32)
    }
    pub fn is_evm(self) -> bool {
        matches!(self, Self::Ethereum | Self::Bnb)
    }
    pub fn wif_version(self) -> Option<u8> {
        match self {
            Self::Legacy | Self::Bech32 => Some(0x80),
            Self::Dogecoin => Some(0x9e),
            Self::Litecoin => Some(0xb0),
            _ => None,
        }
    }
    pub fn base58_version(self) -> Option<u8> {
        match self {
            Self::Legacy | Self::Xrp => Some(0),
            Self::Dogecoin => Some(0x1e),
            Self::Litecoin => Some(0x30),
            Self::Tron => Some(0x41),
            _ => None,
        }
    }
    pub fn lengths(self) -> std::ops::RangeInclusive<usize> {
        match self {
            Self::Legacy | Self::Xrp => 25..=34,
            Self::Bech32 | Self::Ethereum | Self::Bnb => 42..=42,
            Self::Solana => 32..=44,
            Self::Tron | Self::Dogecoin | Self::Litecoin => 34..=34,
        }
    }
    pub fn chain_name(self) -> &'static str {
        match self {
            Self::Legacy | Self::Bech32 => "bitcoin",
            Self::Ethereum => "ethereum",
            Self::Solana => "solana",
            Self::Bnb => "bnb",
            Self::Xrp => "xrp",
            Self::Tron => "tron",
            Self::Dogecoin => "dogecoin",
            Self::Litecoin => "litecoin",
        }
    }
    pub fn fixed(self) -> &'static str {
        match self {
            Self::Legacy => "1",
            Self::Bech32 => "bc1q",
            Self::Ethereum | Self::Bnb => "0x",
            Self::Solana => "",
            Self::Xrp => "r",
            Self::Tron => "T",
            Self::Dogecoin => "D",
            Self::Litecoin => "L",
        }
    }
    pub fn alphabet(self) -> &'static [u8] {
        match self {
            Self::Legacy | Self::Solana | Self::Tron | Self::Dogecoin | Self::Litecoin => {
                b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
            }
            Self::Bech32 => b"qpzry9x8gf2tvdw0s3jn54khce6mua7l",
            Self::Ethereum | Self::Bnb => b"0123456789abcdefABCDEF",
            Self::Xrp => b"rpshnaf39wBUDNEGHJKLM4PQRST7VWXYZ2bcdeCg65jkm8oFqi1tuvAxyz",
        }
    }
}

pub fn hash160(public: &PublicKey) -> [u8; 20] {
    Ripemd160::digest(Sha256::digest(public.serialize())).into()
}

pub fn derive(public: &PublicKey, kind: AddressType) -> String {
    if kind.is_evm() {
        return crate::ethereum::derive(public);
    }
    if let Some(version) = kind.base58_version() {
        let hash = if kind == AddressType::Tron {
            crate::ethereum::account_id(public)
        } else {
            hash160(public)
        };
        let mut bytes = [0u8; 25];
        bytes[0] = version;
        bytes[1..21].copy_from_slice(&hash);
        let checksum = Sha256::digest(Sha256::digest(&bytes[..21]));
        bytes[21..].copy_from_slice(&checksum[..4]);
        let alphabet = if kind == AddressType::Xrp {
            bs58::Alphabet::RIPPLE
        } else {
            bs58::Alphabet::BITCOIN
        };
        return bs58::encode(bytes).with_alphabet(alphabet).into_string();
    }
    let hash = hash160(public);
    match kind {
        AddressType::Bech32 => bech32::segwit::encode_v0(bech32::hrp::BC, &hash)
            .expect("20-byte P2WPKH program is valid"),
        AddressType::Solana => panic!("Solana requires an Ed25519 public key"),
        _ => unreachable!("handled above"),
    }
}

pub fn wif(secret: &[u8; 32]) -> Zeroizing<String> {
    wif_with_version(secret, 0x80)
}

pub fn wif_with_version(secret: &[u8; 32], version: u8) -> Zeroizing<String> {
    let mut bytes = Zeroizing::new([0u8; 38]);
    bytes[0] = version;
    bytes[1..33].copy_from_slice(secret);
    bytes[33] = 1;
    let mut checksum = Zeroizing::new([0u8; 32]);
    checksum.copy_from_slice(&Sha256::digest(Sha256::digest(&bytes[..34])));
    bytes[34..].copy_from_slice(&checksum[..4]);
    // Preallocation prevents secret-bearing String reallocations.
    let mut result = Zeroizing::new(String::with_capacity(52));
    bs58::encode(&bytes[..])
        .onto(&mut *result)
        .expect("String is growable");
    result
}

pub fn hex(bytes: &[u8]) -> Zeroizing<String> {
    use std::fmt::Write;
    let mut out = Zeroizing::new(String::with_capacity(bytes.len() * 2));
    for byte in bytes {
        write!(out, "{byte:02x}").expect("String writing is infallible");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use secp256k1::{Secp256k1, SecretKey};

    #[test]
    fn xrpl_published_keypair_vector() {
        // https://github.com/XRPLF/xrpl.js/blob/main/packages/ripple-keypairs/test/fixtures/api.json
        let mut key: SecretKey = "D78B9735C3F26501C7337B8A5727FD53A6EFDBC6AA55984F098488561F985E23"
            .parse()
            .unwrap();
        let public = PublicKey::from_secret_key(&Secp256k1::new(), &key);
        key.non_secure_erase();
        assert_eq!(
            &*hex(&public.serialize()),
            "030d58eb48b4420b1f7b9df55087e0e29fef0e8468f9a6825b01ca2c361042d435"
        );
        assert_eq!(
            derive(&public, AddressType::Xrp),
            "rU6K7V3Po4snVhBBaU29sesqs2qTQJWDw1"
        );
    }

    #[test]
    fn dogecoin_and_litecoin_core_vectors() {
        // Mainnet compressed-key fixtures from each project's src/test/key_tests.cpp:
        // https://github.com/dogecoin/dogecoin/blob/master/src/test/key_tests.cpp
        // https://github.com/litecoin-project/litecoin/blob/master/src/test/key_tests.cpp
        for (kind, wif_text, expected) in [
            (
                AddressType::Dogecoin,
                "QP8WvtVMV2iU6y7LE27ksRspp4MAJizPWYovx88W71g1nfSdAhkV",
                "D8jZ6R8uuyQwiybupiVs3eDCedKdZ5bYV3",
            ),
            (
                AddressType::Dogecoin,
                "QTuro8Pwx5yaonvJmU4jbBfwuEmTViyAGNeNyfnG82o7HWJmnrLj",
                "DP7rGcDbpAvMb1dKup981zNt1heWUuVLP7",
            ),
            (
                AddressType::Litecoin,
                "T3gJYmBuZXsdd65E7NQF88ZmUP2MaUanqnZg9GFS94W7kND4Ebjq",
                "Lh2G82Bi33RNuzz4UfSMZbh54jnWHVnmw8",
            ),
            (
                AddressType::Litecoin,
                "T986ZKRRdnuuXLeDZuKBRrZW1ujotAncU9WTrFU1n7vMgRW75ZtF",
                "LWegHWHB5rmaF5rgWYt1YN3StapRdnGJfU",
            ),
        ] {
            let decoded = bitcoin::base58::decode_check(wif_text).unwrap();
            assert_eq!(decoded.len(), 34);
            assert_eq!(decoded[0], kind.wif_version().unwrap());
            assert_eq!(decoded[33], 1);
            let secret: [u8; 32] = decoded[1..33].try_into().unwrap();
            let mut key = SecretKey::from_byte_array(secret).unwrap();
            let public = PublicKey::from_secret_key(&Secp256k1::new(), &key);
            key.non_secure_erase();
            assert_eq!(derive(&public, kind), expected);
            assert_eq!(
                &*wif_with_version(&secret, kind.wif_version().unwrap()),
                wif_text
            );
        }
    }

    #[test]
    fn bnb_uses_evm_address_and_checksum() {
        let mut key: SecretKey = "289c2857d4598e37fb9647507e47a309d6133539bf21a8b9cb6df88fd5232032"
            .parse()
            .unwrap();
        let public = PublicKey::from_secret_key(&Secp256k1::new(), &key);
        key.non_secure_erase();
        assert_eq!(
            derive(&public, AddressType::Bnb),
            derive(&public, AddressType::Ethereum)
        );
        assert_eq!(
            derive(&public, AddressType::Bnb).to_ascii_lowercase(),
            "0x970e8128ab834e8eac17ab8e3812f010678cf791"
        );
    }

    #[test]
    fn java_tron_key_and_address_vector() {
        // java-tron develop: framework/src/test/java/org/tron/common/crypto/ECKeyTest.java
        // The scalar is generateOccupationConstantPrivateKey() in client/utils/AbiUtil.java.
        let mut key: SecretKey = "1234567890123456789012345678901234567890123456789012345678901234"
            .parse()
            .unwrap();
        let public = PublicKey::from_secret_key(&Secp256k1::new(), &key);
        key.non_secure_erase();
        assert_eq!(
            &*hex(&public.serialize()),
            "02e90c7d3640a1568839c31b70a893ab6714ef8415b9de90cedfc1c8f353a6983e"
        );
        let address = derive(&public, AddressType::Tron);
        assert_eq!(address.len(), 34);
        assert!(address.starts_with('T'));
        let decoded = bitcoin::base58::decode_check(&address).unwrap();
        assert_eq!(
            &*hex(&decoded),
            "412e988a386a799f506693793c6a5af6b54dfaabfb"
        );
    }

    #[test]
    fn documented_legacy_vector() {
        // https://en.bitcoin.it/wiki/Technical_background_of_Bitcoin_addresses
        let mut key: SecretKey = "18e14a7b6a307f426a94f8114701e7c8e774e7f9a47e2c2035db29a206321725"
            .parse()
            .unwrap();
        let public = PublicKey::from_secret_key(&Secp256k1::new(), &key);
        assert_eq!(
            &*hex(&public.serialize()),
            "0250863ad64a87ae8a2fe83c1af1a8403cb53f53e486d8511dad8a04887e5b2352"
        );
        assert_eq!(
            derive(&public, AddressType::Legacy),
            "1PMycacnJaSqwwJqjawXBErnLsZ7RkXUAs"
        );
        key.non_secure_erase();
    }

    #[test]
    fn documented_bip173_vector() {
        // BIP173 examples use G, i.e. private scalar 1.
        // https://github.com/bitcoin/bips/blob/master/bip-0173.mediawiki#examples
        let mut bytes = [0; 32];
        bytes[31] = 1;
        let mut key = SecretKey::from_byte_array(bytes).unwrap();
        let public = PublicKey::from_secret_key(&Secp256k1::new(), &key);
        assert_eq!(
            derive(&public, AddressType::Bech32),
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"
        );
        assert_eq!(
            derive(&public, AddressType::Legacy),
            "1BgGZ9tcN4rm9KBzDn7KprQz87SZ26SAMH"
        );
        assert_eq!(
            &*wif(&bytes),
            "KwDiBf89QgGbjEhKnhXJuH7LrciVrZi3qYjgd9M7rFU73sVHnoWn"
        );
        key.non_secure_erase();
    }
}
