pub enum Base {
    Decimal,
    Binary,
}

static GENERIC_DECIMAL_PREFIXES: &'static [&'static str] =
    &["", "k", "M", "G", "T", "P", "E", "Z", "Y"];

static GENERIC_BINARY_PREFIXES: &'static [&'static str] =
    &["", "Ki", "Mi", "Gi", "Ti", "Pi", "Ei", "Zi", "Yi"];

pub fn to_largest_unit(amount: f64, prefix_base: Base) -> (f64, &'static str) {
    let mut x = amount.clone();
    let (prefixes, base) = match prefix_base {
        Base::Decimal => (GENERIC_DECIMAL_PREFIXES, 1000.0),
        Base::Binary => (GENERIC_BINARY_PREFIXES, 1024.0),
    };
    for unit in prefixes {
        if x < base {
            return (x, unit.clone());
        }
        x /= base;
    }
    return (x, prefixes[prefixes.len() - 1]);
}
