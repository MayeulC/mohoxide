// RTS-CORE-ENGINE DETERMINISM: transcendental math that is bit-identical on every
// platform. The C library's sin/pow/log/... differ between Apple's libm and
// Microsoft's CRT (last-bit rounding), which lockstep peers on different
// platforms would see as a desync. These are implemented in Rust by the
// engine (`src/engine/lua/det_math.rs`, the `libm` crate: a port of musl's
// libm that uses only IEEE-exact operations) and resolved at link time.
#pragma once

extern "C"
{
    double rts_det_sin(double x);
    double rts_det_cos(double x);
    double rts_det_tan(double x);
    double rts_det_asin(double x);
    double rts_det_acos(double x);
    double rts_det_atan(double x);
    double rts_det_atan2(double y, double x);
    double rts_det_sinh(double x);
    double rts_det_cosh(double x);
    double rts_det_tanh(double x);
    double rts_det_exp(double x);
    double rts_det_log(double x);
    double rts_det_log2(double x);
    double rts_det_log10(double x);
    double rts_det_pow(double x, double y);
}
