use crate::pattern::Pattern;

pub struct Estimate {
    pub space: f64,
    pub half_space: f64,
}

impl Estimate {
    pub fn new(pattern: &Pattern) -> Self {
        // Count overlapping characters once, using the longest compatible length.
        let slots = pattern
            .kind
            .lengths()
            .rev()
            .find_map(|n| pattern.constraints(n))
            .expect("validated pattern");
        let mut space = 1.0;
        for c in slots.iter().skip(pattern.kind.fixed().len()).flatten() {
            space *= pattern.character_cost(*c);
        }
        Self {
            space,
            half_space: space / 2.0,
        }
    }
    pub fn mean_seconds(&self, rate: f64) -> f64 {
        self.space / rate
    }
}

pub fn duration(seconds: f64) -> String {
    if !seconds.is_finite() || seconds > 86400.0 * 365.25 * 1e6 {
        return format!("{:.2e} years", seconds / (86400.0 * 365.25));
    }
    if seconds >= 86400.0 {
        format!("{:.2} days", seconds / 86400.0)
    } else if seconds >= 3600.0 {
        format!("{:.2} hours", seconds / 3600.0)
    } else if seconds >= 60.0 {
        format!("{:.2} min", seconds / 60.0)
    } else {
        format!("{seconds:.2} s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::address::AddressType;
    #[test]
    fn ethereum_case_weights_and_solana_overlap() {
        let p = Pattern::new("a0A".into(), "".into(), false, AddressType::Ethereum).unwrap();
        assert_eq!(Estimate::new(&p).space, 32.0 * 16.0 * 32.0);
        let p = Pattern::new("a0A".into(), "".into(), true, AddressType::Ethereum).unwrap();
        assert_eq!(Estimate::new(&p).space, 16f64.powi(3));
        let p = Pattern::new("a0".repeat(20), "a0".into(), true, AddressType::Ethereum).unwrap();
        assert_eq!(Estimate::new(&p).space, 16f64.powi(40));
        let p = Pattern::new("A".into(), "7".into(), true, AddressType::Solana).unwrap();
        assert_eq!(Estimate::new(&p).space, 29.0 * 58.0);
    }
    #[test]
    fn alphabet_case_and_overlap() {
        let p = Pattern::new("Aa".into(), "7".into(), false, AddressType::Legacy).unwrap();
        assert_eq!(Estimate::new(&p).space, 58f64.powi(3));
        let p = Pattern::new("Aa".into(), "7".into(), true, AddressType::Legacy).unwrap();
        assert_eq!(Estimate::new(&p).space, 29.0 * 29.0 * 58.0);
        let p = Pattern::new("q".into(), "p".into(), true, AddressType::Bech32).unwrap();
        assert_eq!(Estimate::new(&p).space, 1024.0);
    }
}
