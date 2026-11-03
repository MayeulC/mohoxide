//! Deterministic transcendental math for the vendored Luau
//! (`vendor/luau0-src/luau/Common/include/Luau/DetMath.h`).
//!
//! Luau called the C library's `sin`, `pow`, `log`, ... for `math.*`, the
//! `^` operator and compile-time constant folding. Apple's libm and
//! Microsoft's CRT round some results differently in the last bit, so a Mac
//! and a Windows peer would drift apart through any AI or unit script that
//! uses them. The `libm` crate (a port of musl's libm) is built from IEEE
//! basic operations only, which are exact on every platform, and Rust never
//! fuses a multiply and an add on its own: bit-identical everywhere.
//! (`sqrt` stays the hardware instruction: IEEE requires it correctly
//! rounded.)

macro_rules! unary {
    ($($c:ident => $f:ident),* $(,)?) => {$(
        #[unsafe(no_mangle)]
        pub extern "C" fn $c(x: f64) -> f64 {
            libm::$f(x)
        }
    )*};
}

unary! {
    rts_det_sin => sin,
    rts_det_cos => cos,
    rts_det_tan => tan,
    rts_det_asin => asin,
    rts_det_acos => acos,
    rts_det_atan => atan,
    rts_det_sinh => sinh,
    rts_det_cosh => cosh,
    rts_det_tanh => tanh,
    rts_det_exp => exp,
    rts_det_log => log,
    rts_det_log2 => log2,
    rts_det_log10 => log10,
}

#[unsafe(no_mangle)]
pub extern "C" fn rts_det_atan2(y: f64, x: f64) -> f64 {
    libm::atan2(y, x)
}

#[unsafe(no_mangle)]
pub extern "C" fn rts_det_pow(x: f64, y: f64) -> f64 {
    libm::pow(x, y)
}

#[cfg(test)]
mod tests {
    use mlua::Lua;

    /// Luau's math library, `^` and constant folding go through these.
    /// The expected bits come from musl's algorithms, so they are the same
    /// on every platform; a platform libm would differ on some of them.
    #[test]
    fn luau_math_uses_the_deterministic_implementations() {
        let lua = Lua::new();
        let bits = |code: &str| lua.load(code).eval::<f64>().unwrap().to_bits();
        for (code, want) in [
            ("local x = ... or 1e22 return math.sin(x)", libm::sin(1e22)),
            ("local x = 0.7 return math.atan2(x, 1.3)", libm::atan2(0.7, 1.3)),
            ("local x = 1.0001 return x ^ 1234.5", libm::pow(1.0001, 1234.5)),
            ("return 1.0001 ^ 1234.5", libm::pow(1.0001, 1234.5)),
            ("return math.exp(0.3) + math.log(7.1) + math.log10(3.3)", libm::exp(0.3) + libm::log(7.1) + libm::log10(3.3)),
        ] {
            assert_eq!(bits(code), want.to_bits(), "{code}");
        }
    }
}
