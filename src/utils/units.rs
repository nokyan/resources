pub enum Base {
    Decimal,
    Binary,
}

static GENERIC_DECIMAL_PREFIXES: &[&str] = &["", "k", "M", "G", "T", "P", "E", "Z", "Y"];

static GENERIC_BINARY_PREFIXES: &[&str] = &["", "Ki", "Mi", "Gi", "Ti", "Pi", "Ei", "Zi", "Yi"];

pub fn to_largest_unit(amount: f64, prefix_base: Base) -> (f64, &'static str) {
    let mut x = amount;
    let (prefixes, base) = match prefix_base {
        Base::Decimal => (GENERIC_DECIMAL_PREFIXES, 1000.0),
        Base::Binary => (GENERIC_BINARY_PREFIXES, 1024.0),
    };
    for unit in prefixes {
        if x < base {
            return (x, *unit);
        }
        x /= base;
    }
    (x, prefixes[prefixes.len() - 1])
}

pub fn celsius_to_fahrenheit(c: f64) -> f64 {
    c * 1.8 + 32.0
}
