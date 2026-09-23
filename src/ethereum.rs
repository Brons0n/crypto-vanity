//! Ethereum externally owned accounts: Keccak-256 of the 64-byte public point,
//! followed by the last 20 bytes and the EIP-55 mixed-case checksum.
use secp256k1::PublicKey;
use sha3::{Digest, Keccak256};

pub fn derive(public: &PublicKey) -> String {
    let lower = crate::address::hex(&account_id(public));
    checksum_hex(&lower)
}

/// Shared 20-byte account identifier for Ethereum, BSC, and TRON.
pub fn account_id(public: &PublicKey) -> [u8; 20] {
    let digest = Keccak256::digest(&public.serialize_uncompressed()[1..]);
    digest[12..].try_into().expect("20-byte digest suffix")
}

/// `lower` must be exactly 40 lowercase hexadecimal characters (public address data).
pub fn checksum_hex(lower: &str) -> String {
    assert!(
        lower.len() == 40
            && lower
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    let digest = Keccak256::digest(lower.as_bytes());
    let mut address = String::with_capacity(42);
    address.push_str("0x");
    for (i, c) in lower.bytes().enumerate() {
        let nibble = if i % 2 == 0 {
            digest[i / 2] >> 4
        } else {
            digest[i / 2] & 15
        };
        address.push(if nibble >= 8 {
            c.to_ascii_uppercase()
        } else {
            c
        } as char);
    }
    address
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn go_ethereum_private_key_vector() {
        // https://github.com/ethereum/go-ethereum/blob/master/crypto/crypto_test.go
        let mut secret: secp256k1::SecretKey =
            "289c2857d4598e37fb9647507e47a309d6133539bf21a8b9cb6df88fd5232032"
                .parse()
                .unwrap();
        let public = PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &secret);
        secret.non_secure_erase();
        assert_eq!(
            derive(&public).to_ascii_lowercase(),
            "0x970e8128ab834e8eac17ab8e3812f010678cf791"
        );
    }
    #[test]
    fn published_eip55_vectors() {
        // https://eips.ethereum.org/EIPS/eip-55
        for expected in [
            "0x52908400098527886E0F7030069857D2E4169EE7",
            "0x8617E340B3D01FA5F11F306F4090FD50E238070D",
            "0xde709f2102306220921060314715629080e2fb77",
            "0x27b1fdb04752bbc536007a920d24acb045561c26",
            "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed",
            "0xfB6916095ca1df60bB79Ce92cE3Ea74c37c5d359",
            "0xdbF03B407c01E7cD3CBea99509d93f8DDDC8C6FB",
            "0xD1220A0cf47c7B9Be7A2E6BA89F429762e7b9aDb",
        ] {
            assert_eq!(checksum_hex(&expected[2..].to_ascii_lowercase()), expected);
        }
    }
}
