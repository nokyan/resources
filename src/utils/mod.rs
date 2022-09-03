pub mod cpu;
pub mod daemon_proxy;
pub mod memory;
pub mod units;

pub trait NaNDefault {
    /// Returns the given `default` value if the variable is NaN,
    /// and returns itself otherwise.
    fn nan_default(&self, default: Self) -> Self;
}

impl NaNDefault for f64 {
    fn nan_default(&self, default: Self) -> Self {
        match self.is_nan() {
            false => *self,
            true => default,
        }
    }
}

impl NaNDefault for f32 {
    fn nan_default(&self, default: Self) -> Self {
        match self.is_nan() {
            false => *self,
            true => default,
        }
    }
}
