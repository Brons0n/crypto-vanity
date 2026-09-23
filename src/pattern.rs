use crate::address::AddressType;
use anyhow::{bail, Result};

#[derive(Clone)]
pub struct Pattern {
    pub prefix: String,
    pub suffix: String,
    pub insensitive: bool,
    pub kind: AddressType,
}

impl Pattern {
    pub fn new(
        prefix: String,
        suffix: String,
        insensitive: bool,
        kind: AddressType,
    ) -> Result<Self> {
        let p = Self {
            prefix,
            suffix,
            insensitive,
            kind,
        };
        let max_address = *kind.lengths().end();
        let max_body = max_address - kind.fixed().len();
        if p.prefix.len() > max_body || p.suffix.len() > max_address {
            bail!(
                "prefix exceeds {max_body} characters or suffix exceeds {max_address} characters"
            );
        }
        for (name, value) in [("prefix", &p.prefix), ("suffix", &p.suffix)] {
            if kind == AddressType::Bech32 && value.bytes().any(|c| c.is_ascii_uppercase()) {
                bail!("bech32 {name} must be lowercase; uppercase and mixed-case patterns are rejected, even with --case-insensitive");
            }
            // A suffix can span the full address, including its fixed leading part.
            let fixed_overlap = if name == "suffix" {
                value.len().saturating_sub(max_body)
            } else {
                0
            };
            for (index, c) in value.bytes().enumerate() {
                if index < fixed_overlap {
                    let expected =
                        kind.fixed().as_bytes()[kind.fixed().len() - fixed_overlap + index];
                    if !p.equal(c, expected) {
                        bail!("suffix conflicts with the fixed address characters");
                    }
                } else if p.variants(c) == 0 {
                    bail!("{name} contains a character outside the selected address alphabet");
                }
            }
        }
        // An overlap must agree for at least one possible length. Bech32 is fixed-length.
        let lengths = kind.lengths();
        if !lengths.into_iter().any(|n| p.constraints(n).is_some()) {
            bail!("prefix and suffix overlap incompatibly; no address can match");
        }
        if !p.possible_network_range() {
            bail!("pattern cannot match this network's Base58 address range; try a different prefix or a suffix-only search");
        }
        // A fully specified bech32 address must have a valid checksum.
        if kind == AddressType::Bech32 {
            if let Some(constraints) = p.constraints(42) {
                if constraints.iter().all(Option::is_some) {
                    let s: String = constraints.iter().map(|c| c.unwrap() as char).collect();
                    if bech32::segwit::decode(&s).is_err() {
                        bail!("fully specified bech32 pattern has an invalid checksum");
                    }
                }
            }
        }
        if kind.is_evm() && !insensitive {
            if let Some(constraints) = p.constraints(42) {
                if constraints.iter().all(Option::is_some) {
                    let s: String = constraints.iter().map(|c| c.unwrap() as char).collect();
                    if crate::ethereum::checksum_hex(&s[2..].to_ascii_lowercase()) != s {
                        bail!("fully specified EVM pattern has invalid EIP-55 checksum casing; use --case-insensitive to ignore case");
                    }
                }
            }
        }
        Ok(p)
    }

    // Nonzero network versions constrain more than the first character. Test
    // all case variants against the interval [version || 00.., version || ff..]
    // with a small digit DP. Checksum validity is deliberately not assumed here.
    fn possible_network_range(&self) -> bool {
        let Some(version) = self.kind.base58_version().filter(|v| *v != 0) else {
            return true;
        };
        let mut low = [0u8; 25];
        let mut high = [255u8; 25];
        low[0] = version;
        high[0] = version;
        let low = bs58::encode(low).into_string();
        let high = bs58::encode(high).into_string();
        debug_assert_eq!(low.len(), 34);
        debug_assert_eq!(high.len(), 34);
        let constraints = self
            .constraints(34)
            .expect("validated fixed-length overlap");
        let alphabet = self.kind.alphabet();
        // Bits: already greater than lower bound; already less than upper bound.
        let mut states = [true, false, false, false];
        for (i, constraint) in constraints.iter().enumerate() {
            let lower = alphabet
                .iter()
                .position(|&c| c == low.as_bytes()[i])
                .unwrap();
            let upper = alphabet
                .iter()
                .position(|&c| c == high.as_bytes()[i])
                .unwrap();
            let mut next = [false; 4];
            for (state, &active) in states.iter().enumerate() {
                if !active {
                    continue;
                }
                for (digit, &c) in alphabet.iter().enumerate() {
                    if constraint.is_some_and(|expected| !self.equal(c, expected))
                        || (state & 1 == 0 && digit < lower)
                        || (state & 2 == 0 && digit > upper)
                    {
                        continue;
                    }
                    next[state | usize::from(digit > lower) | (usize::from(digit < upper) << 1)] =
                        true;
                }
            }
            states = next;
        }
        states.iter().any(|&active| active)
    }

    pub fn character_cost(&self, c: u8) -> f64 {
        if self.kind.is_evm() {
            if c.is_ascii_digit() || self.insensitive {
                16.0
            } else {
                32.0
            }
        } else {
            self.kind.alphabet().len() as f64 / self.variants(c) as f64
        }
    }

    pub fn variants(&self, c: u8) -> usize {
        self.kind
            .alphabet()
            .iter()
            .filter(|&&a| self.equal(a, c))
            .count()
    }

    fn equal(&self, a: u8, b: u8) -> bool {
        if self.insensitive {
            a.eq_ignore_ascii_case(&b)
        } else {
            a == b
        }
    }

    pub fn matches(&self, address: &str) -> bool {
        let Some(body) = address.strip_prefix(self.kind.fixed()) else {
            return false;
        };
        body.len() >= self.prefix.len()
            && address.len() >= self.suffix.len()
            && body
                .bytes()
                .zip(self.prefix.bytes())
                .all(|(a, b)| self.equal(a, b))
            && address
                .bytes()
                .rev()
                .zip(self.suffix.bytes().rev())
                .all(|(a, b)| self.equal(a, b))
    }

    pub fn constraints(&self, len: usize) -> Option<Vec<Option<u8>>> {
        let fixed = self.kind.fixed();
        if fixed.len() + self.prefix.len() > len || self.suffix.len() > len {
            return None;
        }
        let mut slots = vec![None; len];
        for (start, text) in [
            (0, fixed),
            (fixed.len(), self.prefix.as_str()),
            (len - self.suffix.len(), self.suffix.as_str()),
        ] {
            for (i, c) in text.bytes().enumerate() {
                if let Some(old) = slots[start + i] {
                    if !self.equal(old, c) {
                        return None;
                    }
                }
                slots[start + i] = Some(c);
            }
        }
        Some(slots)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn network_range_accepts_real_addresses_and_case_variants() {
        let secp = secp256k1::Secp256k1::new();
        for scalar in 1..=64 {
            let mut bytes = [0u8; 32];
            bytes[31] = scalar;
            let mut key = secp256k1::SecretKey::from_byte_array(bytes).unwrap();
            let public = secp256k1::PublicKey::from_secret_key(&secp, &key);
            key.non_secure_erase();
            for kind in [
                AddressType::Xrp,
                AddressType::Tron,
                AddressType::Dogecoin,
                AddressType::Litecoin,
            ] {
                let address = crate::address::derive(&public, kind);
                for insensitive in [false, true] {
                    let requested = if insensitive {
                        address.to_ascii_lowercase()
                    } else {
                        address.clone()
                    };
                    let p =
                        Pattern::new(requested[1..].into(), requested.clone(), insensitive, kind)
                            .unwrap();
                    assert!(p.matches(&address));
                    assert!(Pattern::new("".into(), requested, insensitive, kind)
                        .unwrap()
                        .matches(&address));
                }
            }
        }
        for kind in [
            AddressType::Tron,
            AddressType::Dogecoin,
            AddressType::Litecoin,
        ] {
            assert!(Pattern::new("zz".into(), "".into(), false, kind).is_err());
            assert!(Pattern::new("11".into(), "".into(), true, kind).is_err());
            assert!(Pattern::new("".into(), "zz".into(), false, kind).is_ok());
        }
    }

    #[test]
    fn bnb_eip55_validation_and_estimates() {
        let address = "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed";
        assert!(
            Pattern::new(address[2..].into(), "".into(), false, AddressType::Bnb)
                .unwrap()
                .matches(address)
        );
        assert!(Pattern::new(
            address[2..].to_ascii_lowercase(),
            "".into(),
            false,
            AddressType::Bnb
        )
        .is_err());
        let p = Pattern::new("a0A".into(), "".into(), true, AddressType::Bnb).unwrap();
        assert_eq!(crate::estimate::Estimate::new(&p).space, 16f64.powi(3));
        let p = Pattern::new("a0A".into(), "".into(), false, AddressType::Bnb).unwrap();
        assert_eq!(crate::estimate::Estimate::new(&p).space, 32.0 * 16.0 * 32.0);
    }
    #[test]
    fn ethereum_and_solana_rules() {
        let eth = "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed";
        assert!(
            Pattern::new(eth[2..].into(), eth.into(), false, AddressType::Ethereum)
                .unwrap()
                .matches(eth)
        );
        assert!(Pattern::new(
            eth[2..].to_lowercase(),
            "".into(),
            false,
            AddressType::Ethereum
        )
        .is_err());
        assert!(
            Pattern::new("5aae".into(), "aed".into(), true, AddressType::Ethereum)
                .unwrap()
                .matches(eth)
        );
        assert!(
            !Pattern::new("5aae".into(), "".into(), false, AddressType::Ethereum)
                .unwrap()
                .matches(eth)
        );
        assert!(Pattern::new("0xabc".into(), "".into(), true, AddressType::Ethereum).is_err());
        assert!(Pattern::new("A".repeat(41), "".into(), false, AddressType::Ethereum).is_err());
        assert!(Pattern::new("".into(), "g".into(), false, AddressType::Ethereum).is_err());
        let sol = "FVen3X669xLzsi6N2V91DoiyzHzg1uAgqiT8jZ9nS96Z";
        assert!(
            Pattern::new("FVen".into(), "96Z".into(), false, AddressType::Solana)
                .unwrap()
                .matches(sol)
        );
        assert!(
            Pattern::new("fven".into(), "96z".into(), true, AddressType::Solana)
                .unwrap()
                .matches(sol)
        );
        assert!(
            !Pattern::new("ven".into(), "".into(), true, AddressType::Solana)
                .unwrap()
                .matches(sol)
        );
        assert!(Pattern::new("A".repeat(45), "".into(), false, AddressType::Solana).is_err());
    }
    #[test]
    fn full_address_suffix_and_checksum() {
        for (kind, address) in [
            (AddressType::Legacy, "1BgGZ9tcN4rm9KBzDn7KprQz87SZ26SAMH"),
            (
                AddressType::Bech32,
                "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4",
            ),
        ] {
            let p = Pattern::new(
                address[kind.fixed().len()..].into(),
                address.into(),
                false,
                kind,
            )
            .unwrap();
            assert!(p.matches(address));
        }
        assert!(Pattern::new(
            "".into(),
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3tq".into(),
            false,
            AddressType::Bech32
        )
        .is_err());
    }
    #[test]
    fn validation_and_matching() {
        assert!(Pattern::new("0".into(), "".into(), false, AddressType::Legacy).is_err());
        assert!(Pattern::new("wQ".into(), "".into(), true, AddressType::Bech32).is_err());
        assert!(Pattern::new("é".into(), "".into(), true, AddressType::Legacy).is_err());
        let p = Pattern::new("bG".into(), "AMH".into(), true, AddressType::Legacy).unwrap();
        assert!(p.matches("1BgGZ9tcN4rm9KBzDn7KprQz87SZ26SAMH"));
        assert!(!p.matches("1ZgGZ9tcN4rm9KBzDn7KprQz87SZ26SAMH"));
        assert!(Pattern::new("q".repeat(38), "p".into(), false, AddressType::Bech32).is_err());
    }
}
