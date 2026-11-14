// SPDX-License-Identifier: AGPL-3.0-only
// The following is adapted from fdlibm (http://www.netlib.org/fdlibm).
//
// ====================================================
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//
// Developed at SunSoft, a Sun Microsystems, Inc. business.
// Permission to use, copy, modify, and distribute this
// software is freely granted, provided that this notice
// is preserved.
// ====================================================
//
// The original source code covered by the above license above has been
// modified significantly by Google Inc.
// Copyright 2016 the V8 project authors. All rights reserved.
//
// Ported from V8 src/base/ieee754.cc, Copyright 2016 the V8 project authors, BSD-3-Clause.
// See ThirdPartyNotices/V8.md.

//! Port of V8 12.4 `src/base/ieee754.cc` (fdlibm) and the `Math.hypot` builtin, bit-exact with Node 22.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Newer V8 versions replaced fdlibm by LLVM libc routines; those results can differ in the last bit.

/// High 32 bits of a double, as signed.
fn high_word(x: f64) -> i32 {
    (x.to_bits() >> 32) as u32 as i32
}

/// Low 32 bits of a double.
fn low_word(x: f64) -> u32 {
    x.to_bits() as u32
}

/// Builds a double from its high and low words.
fn from_words(hi: u32, lo: u32) -> f64 {
    f64::from_bits(((hi as u64) << 32) | lo as u64)
}

/// Replaces the high 32 bits of a double.
fn with_high(x: f64, hi: u32) -> f64 {
    from_words(hi, low_word(x))
}

/// Replaces the low 32 bits of a double.
fn with_low(x: f64, lo: u32) -> f64 {
    f64::from_bits((x.to_bits() & 0xFFFF_FFFF_0000_0000) | lo as u64)
}

/// `x * 2^n` with correct rounding, like C `scalbn`.
fn scalbn(x: f64, n: i32) -> f64 {
    let p1023 = f64::from_bits(0x7FE0_0000_0000_0000);
    let pm969 = f64::from_bits(0x0360_0000_0000_0000); // 2^-1022 * 2^53
    let mut y = x;
    let mut n = n;
    if n > 1023 {
        y *= p1023;
        n -= 1023;
        if n > 1023 {
            y *= p1023;
            n -= 1023;
            n = n.min(1023);
        }
    } else if n < -1022 {
        y *= pm969;
        n += 1022 - 53;
        if n < -1022 {
            y *= pm969;
            n += 1022 - 53;
            n = n.max(-1022);
        }
    }
    y * from_words(((0x3FF + n) as u32) << 20, 0)
}

const TWO_OVER_PI: [i32; 66] = [
    0xA2F983, 0x6E4E44, 0x1529FC, 0x2757D1, 0xF534DD, 0xC0DB62, 0x95993C, 0x439041, 0xFE5163,
    0xABDEBB, 0xC561B7, 0x246E3A, 0x424DD2, 0xE00649, 0x2EEA09, 0xD1921C, 0xFE1DEB, 0x1CB129,
    0xA73EE8, 0x8235F5, 0x2EBB44, 0x84E99C, 0x7026B4, 0x5F7E41, 0x3991D6, 0x398353, 0x39F49C,
    0x845F8B, 0xBDF928, 0x3B1FF8, 0x97FFDE, 0x05980F, 0xEF2F11, 0x8B5A0A, 0x6D1F6D, 0x367ECF,
    0x27CB09, 0xB74F46, 0x3F669E, 0x5FEA2D, 0x7527BA, 0xC7EBE5, 0xF17B3D, 0x0739F7, 0x8A5292,
    0xEA6BFB, 0x5FB11F, 0x8D5D08, 0x560330, 0x46FC7B, 0x6BABF0, 0xCFBC20, 0x9AF436, 0x1DA9E3,
    0x91615E, 0xE61B08, 0x659985, 0x5F14A0, 0x68408D, 0xFFD880, 0x4D7327, 0x310606, 0x1556CA,
    0x73A8C9, 0x60E27B, 0xC08C6B,
];

const NPIO2_HW: [i32; 32] = [
    0x3FF921FB, 0x400921FB, 0x4012D97C, 0x401921FB, 0x401F6A7A, 0x4022D97C, 0x4025FDBB, 0x402921FB,
    0x402C463A, 0x402F6A7A, 0x4031475C, 0x4032D97C, 0x40346B9C, 0x4035FDBB, 0x40378FDB, 0x403921FB,
    0x403AB41B, 0x403C463A, 0x403DD85A, 0x403F6A7A, 0x40407E4C, 0x4041475C, 0x4042106C, 0x4042D97C,
    0x4043A28C, 0x40446B9C, 0x404534AC, 0x4045FDBB, 0x4046C6CB, 0x40478FDB, 0x404858EB, 0x404921FB,
];

const PIO2: [f64; 8] = [
    1.57079625129699707031e+00,
    7.54978941586159635335e-08,
    5.39030252995776476554e-15,
    3.28200341580791294123e-22,
    1.27065575308067607349e-29,
    1.22933308981111328932e-36,
    2.73370053816464559624e-44,
    2.16741683877804819444e-51,
];

/// `__kernel_rem_pio2` for 53-bit precision (prec = 2); returns `n & 7` and fills `y`.
fn kernel_rem_pio2(x: &[f64; 3], y: &mut [f64; 2], e0: i32, nx: i32) -> i32 {
    let two24 = 1.67772160000000000000e+07;
    let twon24 = 5.96046447753906250000e-08;
    let jk: i32 = 4;
    let jp = jk;
    let mut iq = [0i32; 20];
    let mut f = [0f64; 20];
    let mut fq = [0f64; 20];
    let mut q = [0f64; 20];

    let jx = nx - 1;
    let mut jv = (e0 - 3) / 24;
    if jv < 0 {
        jv = 0;
    }
    let mut q0 = e0 - 24 * (jv + 1);

    let mut j = jv - jx;
    let m = jx + jk;
    for fi in f.iter_mut().take(m as usize + 1) {
        *fi = if j < 0 {
            0.0
        } else {
            TWO_OVER_PI[j as usize] as f64
        };
        j += 1;
    }
    for i in 0..=jk {
        let mut fw = 0.0;
        for j in 0..=jx {
            fw += x[j as usize] * f[(jx + i - j) as usize];
        }
        q[i as usize] = fw;
    }

    let mut jz = jk;
    let mut z;
    let mut n;
    let mut ih;
    loop {
        // distill q[] into iq[] reversingly
        let mut i = 0usize;
        let mut j = jz;
        z = q[jz as usize];
        while j > 0 {
            let fw = ((twon24 * z) as i32) as f64;
            iq[i] = (z - two24 * fw) as i32;
            z = q[(j - 1) as usize] + fw;
            i += 1;
            j -= 1;
        }
        z = scalbn(z, q0);
        z -= 8.0 * (z * 0.125).floor();
        n = z as i32;
        z -= n as f64;
        ih = 0;
        if q0 > 0 {
            let i = iq[(jz - 1) as usize] >> (24 - q0);
            n += i;
            iq[(jz - 1) as usize] -= i << (24 - q0);
            ih = iq[(jz - 1) as usize] >> (23 - q0);
        } else if q0 == 0 {
            ih = iq[(jz - 1) as usize] >> 23;
        } else if z >= 0.5 {
            ih = 2;
        }
        if ih > 0 {
            n += 1;
            let mut carry = 0;
            for item in iq.iter_mut().take(jz as usize) {
                let j = *item;
                if carry == 0 {
                    if j != 0 {
                        carry = 1;
                        *item = 0x1000000 - j;
                    }
                } else {
                    *item = 0xFFFFFF - j;
                }
            }
            if q0 > 0 {
                match q0 {
                    1 => iq[(jz - 1) as usize] &= 0x7FFFFF,
                    2 => iq[(jz - 1) as usize] &= 0x3FFFFF,
                    _ => {}
                }
            }
            if ih == 2 {
                z = 1.0 - z;
                if carry != 0 {
                    z -= scalbn(1.0, q0);
                }
            }
        }
        if z == 0.0 {
            let mut j = 0;
            let mut i = jz - 1;
            while i >= jk {
                j |= iq[i as usize];
                i -= 1;
            }
            if j == 0 {
                let mut k = 1;
                while jk >= k && iq[(jk - k) as usize] == 0 {
                    k += 1;
                }
                for i in (jz + 1)..=(jz + k) {
                    f[(jx + i) as usize] = TWO_OVER_PI[(jv + i) as usize] as f64;
                    let mut fw = 0.0;
                    for j in 0..=jx {
                        fw += x[j as usize] * f[(jx + i - j) as usize];
                    }
                    q[i as usize] = fw;
                }
                jz += k;
                continue;
            }
        }
        break;
    }

    if z == 0.0 {
        jz -= 1;
        q0 -= 24;
        while iq[jz as usize] == 0 {
            jz -= 1;
            q0 -= 24;
        }
    } else {
        z = scalbn(z, -q0);
        if z >= two24 {
            let fw = ((twon24 * z) as i32) as f64;
            iq[jz as usize] = (z - two24 * fw) as i32;
            jz += 1;
            q0 += 24;
            iq[jz as usize] = fw as i32;
        } else {
            iq[jz as usize] = z as i32;
        }
    }

    let mut fw = scalbn(1.0, q0);
    let mut i = jz;
    while i >= 0 {
        q[i as usize] = fw * iq[i as usize] as f64;
        fw *= twon24;
        i -= 1;
    }
    let mut i = jz;
    while i >= 0 {
        let mut fw = 0.0;
        let mut k = 0;
        while k <= jp && k <= jz - i {
            fw += PIO2[k as usize] * q[(i + k) as usize];
            k += 1;
        }
        fq[(jz - i) as usize] = fw;
        i -= 1;
    }

    let mut fw = 0.0;
    let mut i = jz;
    while i >= 0 {
        fw += fq[i as usize];
        i -= 1;
    }
    y[0] = if ih == 0 { fw } else { -fw };
    fw = fq[0] - fw;
    for i in 1..=jz {
        fw += fq[i as usize];
    }
    y[1] = if ih == 0 { fw } else { -fw };
    n & 7
}

/// `__ieee754_rem_pio2`: returns `n` and fills `y` with the remainder of `x` modulo pi/2.
fn rem_pio2(x: f64, y: &mut [f64; 2]) -> i32 {
    let half = 5.00000000000000000000e-01;
    let two24 = 1.67772160000000000000e+07;
    let invpio2 = 6.36619772367581382433e-01;
    let pio2_1 = 1.57079632673412561417e+00;
    let pio2_1t = 6.07710050650619224932e-11;
    let pio2_2 = 6.07710050630396597660e-11;
    let pio2_2t = 2.02226624879595063154e-21;
    let pio2_3 = 2.02226624871116645580e-21;
    let pio2_3t = 8.47842766036889956997e-32;

    let hx = high_word(x);
    let ix = hx & 0x7FFFFFFF;
    if ix <= 0x3FE921FB {
        y[0] = x;
        y[1] = 0.0;
        return 0;
    }
    if ix < 0x4002D97C {
        if hx > 0 {
            let mut z = x - pio2_1;
            if ix != 0x3FF921FB {
                y[0] = z - pio2_1t;
                y[1] = (z - y[0]) - pio2_1t;
            } else {
                z -= pio2_2;
                y[0] = z - pio2_2t;
                y[1] = (z - y[0]) - pio2_2t;
            }
            return 1;
        } else {
            let mut z = x + pio2_1;
            if ix != 0x3FF921FB {
                y[0] = z + pio2_1t;
                y[1] = (z - y[0]) + pio2_1t;
            } else {
                z += pio2_2;
                y[0] = z + pio2_2t;
                y[1] = (z - y[0]) + pio2_2t;
            }
            return -1;
        }
    }
    if ix <= 0x413921FB {
        let t = x.abs();
        let n = (t * invpio2 + half) as i32;
        let fn_ = n as f64;
        let mut r = t - fn_ * pio2_1;
        let mut w = fn_ * pio2_1t;
        if n < 32 && ix != NPIO2_HW[(n - 1) as usize] {
            y[0] = r - w;
        } else {
            let j = ix >> 20;
            y[0] = r - w;
            let high = high_word(y[0]) as u32;
            let i = j - ((high >> 20) & 0x7FF) as i32;
            if i > 16 {
                let t = r;
                w = fn_ * pio2_2;
                r = t - w;
                w = fn_ * pio2_2t - ((t - r) - w);
                y[0] = r - w;
                let high = high_word(y[0]) as u32;
                let i = j - ((high >> 20) & 0x7FF) as i32;
                if i > 49 {
                    let t = r;
                    w = fn_ * pio2_3;
                    r = t - w;
                    w = fn_ * pio2_3t - ((t - r) - w);
                    y[0] = r - w;
                }
            }
        }
        y[1] = (r - y[0]) - w;
        if hx < 0 {
            y[0] = -y[0];
            y[1] = -y[1];
            return -n;
        }
        return n;
    }
    if ix >= 0x7FF00000 {
        y[0] = x - x;
        y[1] = y[0];
        return 0;
    }
    let mut z = with_low(0.0, low_word(x));
    let e0 = (ix >> 20) - 1046;
    z = with_high(
        z,
        ix.wrapping_sub((e0 as u32).wrapping_shl(20) as i32) as u32,
    );
    let mut tx = [0f64; 3];
    for t in tx.iter_mut().take(2) {
        *t = (z as i32) as f64;
        z = (z - *t) * two24;
    }
    tx[2] = z;
    let mut nx = 3;
    while tx[nx - 1] == 0.0 {
        nx -= 1;
    }
    let n = kernel_rem_pio2(&tx, y, e0, nx as i32);
    if hx < 0 {
        y[0] = -y[0];
        y[1] = -y[1];
        return -n;
    }
    n
}

/// `__kernel_cos`.
fn kernel_cos(x: f64, y: f64) -> f64 {
    let c1 = 4.16666666666666019037e-02;
    let c2 = -1.38888888888741095749e-03;
    let c3 = 2.48015872894767294178e-05;
    let c4 = -2.75573143513906633035e-07;
    let c5 = 2.08757232129817482790e-09;
    let c6 = -1.13596475577881948265e-11;
    let ix = high_word(x) & 0x7FFFFFFF;
    if ix < 0x3E400000 && (x as i32) == 0 {
        return 1.0;
    }
    let z = x * x;
    let r = z * (c1 + z * (c2 + z * (c3 + z * (c4 + z * (c5 + z * c6)))));
    if ix < 0x3FD33333 {
        1.0 - (0.5 * z - (z * r - x * y))
    } else {
        let qx = if ix > 0x3FE90000 {
            0.28125
        } else {
            from_words((ix - 0x00200000) as u32, 0)
        };
        let iz = 0.5 * z - qx;
        let a = 1.0 - qx;
        a - (iz - (z * r - x * y))
    }
}

/// `__kernel_sin`.
fn kernel_sin(x: f64, y: f64, iy: i32) -> f64 {
    let half = 5.00000000000000000000e-01;
    let s1 = -1.66666666666666324348e-01;
    let s2 = 8.33333333332248946124e-03;
    let s3 = -1.98412698298579493134e-04;
    let s4 = 2.75573137070700676789e-06;
    let s5 = -2.50507602534068634195e-08;
    let s6 = 1.58969099521155010221e-10;
    let ix = high_word(x) & 0x7FFFFFFF;
    if ix < 0x3E400000 && (x as i32) == 0 {
        return x;
    }
    let z = x * x;
    let v = z * x;
    let r = s2 + z * (s3 + z * (s4 + z * (s5 + z * s6)));
    if iy == 0 {
        x + v * (s1 + z * r)
    } else {
        x - ((z * (half * y - v * r) - y) - v * s1)
    }
}

/// `__kernel_tan`.
fn kernel_tan(x: f64, y: f64, iy: i32) -> f64 {
    const T: [f64; 13] = [
        3.33333333333334091986e-01,
        1.33333333333201242699e-01,
        5.39682539762260521377e-02,
        2.18694882948595424599e-02,
        8.86323982359930005737e-03,
        3.59207910759131235356e-03,
        1.45620945432529025516e-03,
        5.88041240820264096874e-04,
        2.46463134818469906812e-04,
        7.81794442939557092300e-05,
        7.14072491382608190305e-05,
        -1.85586374855275456654e-05,
        2.59073051863633712884e-05,
    ];
    let one = 1.0;
    let pio4 = 7.85398163397448278999e-01;
    let pio4lo = 3.06161699786838301793e-17;
    let (mut x, mut y) = (x, y);
    let hx = high_word(x);
    let ix = hx & 0x7FFFFFFF;
    if ix < 0x3E300000 && (x as i32) == 0 {
        let low = low_word(x);
        if ((ix as u32 | low) | (iy + 1) as u32) == 0 {
            return one / x.abs();
        } else if iy == 1 {
            return x;
        } else {
            let w = x + y;
            let z = with_low(w, 0);
            let v = y - (z - x);
            let a = -one / w;
            let t = with_low(a, 0);
            let s = one + t * z;
            return t + a * (s + t * v);
        }
    }
    if ix >= 0x3FE59428 {
        if hx < 0 {
            x = -x;
            y = -y;
        }
        let z = pio4 - x;
        let w = pio4lo - y;
        x = z + w;
        y = 0.0;
    }
    let z = x * x;
    let w = z * z;
    let mut r = T[1] + w * (T[3] + w * (T[5] + w * (T[7] + w * (T[9] + w * T[11]))));
    let v = z * (T[2] + w * (T[4] + w * (T[6] + w * (T[8] + w * (T[10] + w * T[12])))));
    let s = z * x;
    r = y + z * (s * (r + v) + y);
    r += T[0] * s;
    let w = x + r;
    if ix >= 0x3FE59428 {
        let v = iy as f64;
        return (1 - ((hx >> 30) & 2)) as f64 * (v - 2.0 * (x - (w * w / (w + v) - r)));
    }
    if iy == 1 {
        w
    } else {
        let z = with_low(w, 0);
        let v = r - (z - x);
        let a = -1.0 / w;
        let t = with_low(a, 0);
        let s = 1.0 + t * z;
        t + a * (s + t * v)
    }
}

const PS: [f64; 6] = [
    1.66666666666666657415e-01,
    -3.25565818622400915405e-01,
    2.01212532134862925881e-01,
    -4.00555345006794114027e-02,
    7.91534994289814532176e-04,
    3.47933107596021167570e-05,
];
const QS: [f64; 4] = [
    -2.40339491173441421878e+00,
    2.02094576023350569471e+00,
    -6.88283971605453293030e-01,
    7.70381505559019352791e-02,
];

/// Rational part `p / q` shared by asin and acos, for argument `z = x^2` (or its analogue).
fn asin_p(z: f64) -> f64 {
    z * (PS[0] + z * (PS[1] + z * (PS[2] + z * (PS[3] + z * (PS[4] + z * PS[5])))))
}

/// Denominator shared by asin and acos.
fn asin_q(z: f64) -> f64 {
    1.0 + z * (QS[0] + z * (QS[1] + z * (QS[2] + z * QS[3])))
}

/// `Math.acos`.
pub fn acos(x: f64) -> f64 {
    let pi = 3.14159265358979311600e+00;
    let pio2_hi = 1.57079632679489655800e+00;
    let pio2_lo = 6.12323399573676603587e-17;
    let hx = high_word(x);
    let ix = hx & 0x7FFFFFFF;
    if ix >= 0x3FF00000 {
        let lx = low_word(x);
        if ((ix - 0x3FF00000) as u32 | lx) == 0 {
            return if hx > 0 { 0.0 } else { pi + 2.0 * pio2_lo };
        }
        return f64::NAN;
    }
    if ix < 0x3FE00000 {
        if ix <= 0x3C600000 {
            return pio2_hi + pio2_lo;
        }
        let z = x * x;
        let p = asin_p(z);
        let q = asin_q(z);
        let r = p / q;
        pio2_hi - (x - (pio2_lo - x * r))
    } else if hx < 0 {
        let z = (1.0 + x) * 0.5;
        let p = asin_p(z);
        let q = asin_q(z);
        let s = z.sqrt();
        let r = p / q;
        let w = r * s - pio2_lo;
        pi - 2.0 * (s + w)
    } else {
        let z = (1.0 - x) * 0.5;
        let s = z.sqrt();
        let df = with_low(s, 0);
        let c = (z - df * df) / (s + df);
        let p = asin_p(z);
        let q = asin_q(z);
        let r = p / q;
        let w = r * s + c;
        2.0 * (df + w)
    }
}

/// `Math.asin`.
pub fn asin(x: f64) -> f64 {
    let huge = 1.000e+300;
    let pio2_hi = 1.57079632679489655800e+00;
    let pio2_lo = 6.12323399573676603587e-17;
    let pio4_hi = 7.85398163397448278999e-01;
    let hx = high_word(x);
    let ix = hx & 0x7FFFFFFF;
    let mut t = 0.0;
    if ix >= 0x3FF00000 {
        let lx = low_word(x);
        if ((ix - 0x3FF00000) as u32 | lx) == 0 {
            return x * pio2_hi + x * pio2_lo;
        }
        return f64::NAN;
    } else if ix < 0x3FE00000 {
        if ix < 0x3E400000 {
            if huge + x > 1.0 {
                return x;
            }
        } else {
            t = x * x;
        }
        let p = asin_p(t);
        let q = asin_q(t);
        let w = p / q;
        return x + x * w;
    }
    let w = 1.0 - x.abs();
    t = w * 0.5;
    let mut p = asin_p(t);
    let mut q = asin_q(t);
    let s = t.sqrt();
    if ix >= 0x3FEF3333 {
        let w = p / q;
        t = pio2_hi - (2.0 * (s + s * w) - pio2_lo);
    } else {
        let w = with_low(s, 0);
        let c = (t - w * w) / (s + w);
        let r = p / q;
        p = 2.0 * s * r - (pio2_lo - 2.0 * c);
        q = pio4_hi - 2.0 * w;
        t = pio4_hi - (p - q);
    }
    if hx > 0 { t } else { -t }
}

/// `Math.atan`.
pub fn atan(x: f64) -> f64 {
    const ATANHI: [f64; 4] = [
        4.63647609000806093515e-01,
        7.85398163397448278999e-01,
        9.82793723247329054082e-01,
        1.57079632679489655800e+00,
    ];
    const ATANLO: [f64; 4] = [
        2.26987774529616870924e-17,
        3.06161699786838301793e-17,
        1.39033110312309984516e-17,
        6.12323399573676603587e-17,
    ];
    const AT: [f64; 11] = [
        3.33333333333329318027e-01,
        -1.99999999998764832476e-01,
        1.42857142725034663711e-01,
        -1.11111104054623557880e-01,
        9.09088713343650656196e-02,
        -7.69187620504482999495e-02,
        6.66107313738753120669e-02,
        -5.83357013379057348645e-02,
        4.97687799461593236017e-02,
        -3.65315727442169155270e-02,
        1.62858201153657823623e-02,
    ];
    let one = 1.0;
    let huge = 1.0e300;
    let mut x = x;
    let hx = high_word(x);
    let ix = hx & 0x7FFFFFFF;
    if ix >= 0x44100000 {
        let low = low_word(x);
        if ix > 0x7FF00000 || (ix == 0x7FF00000 && low != 0) {
            return x + x;
        }
        return if hx > 0 {
            ATANHI[3] + ATANLO[3]
        } else {
            -ATANHI[3] - ATANLO[3]
        };
    }
    let id: i32;
    if ix < 0x3FDC0000 {
        if ix < 0x3E400000 && huge + x > one {
            return x;
        }
        id = -1;
    } else {
        x = x.abs();
        if ix < 0x3FF30000 {
            if ix < 0x3FE60000 {
                id = 0;
                x = (2.0 * x - one) / (2.0 + x);
            } else {
                id = 1;
                x = (x - one) / (x + one);
            }
        } else if ix < 0x40038000 {
            id = 2;
            x = (x - 1.5) / (one + 1.5 * x);
        } else {
            id = 3;
            x = -1.0 / x;
        }
    }
    let z = x * x;
    let w = z * z;
    let s1 = z * (AT[0] + w * (AT[2] + w * (AT[4] + w * (AT[6] + w * (AT[8] + w * AT[10])))));
    let s2 = w * (AT[1] + w * (AT[3] + w * (AT[5] + w * (AT[7] + w * AT[9]))));
    if id < 0 {
        x - x * (s1 + s2)
    } else {
        let id = id as usize;
        let z = ATANHI[id] - ((x * (s1 + s2) - ATANLO[id]) - x);
        if hx < 0 { -z } else { z }
    }
}

/// `Math.atan2`.
pub fn atan2(y: f64, x: f64) -> f64 {
    let tiny = 1.0e-300;
    let pi_o_4 = 7.8539816339744827900E-01;
    let pi_o_2 = 1.5707963267948965580E+00;
    let pi = 3.1415926535897931160E+00;
    let pi_lo = 1.2246467991473531772E-16;
    let hx = high_word(x);
    let lx = low_word(x);
    let ix = hx & 0x7FFFFFFF;
    let hy = high_word(y);
    let ly = low_word(y);
    let iy = hy & 0x7FFFFFFF;
    if ((ix as u32 | ((lx | lx.wrapping_neg()) >> 31)) > 0x7FF00000)
        || ((iy as u32 | ((ly | ly.wrapping_neg()) >> 31)) > 0x7FF00000)
    {
        return x + y;
    }
    if (hx.wrapping_sub(0x3FF00000) as u32 | lx) == 0 {
        return atan(y);
    }
    let mut m = ((hy >> 31) & 1) | ((hx >> 30) & 2);
    if (iy as u32 | ly) == 0 {
        return match m {
            0 | 1 => y,
            2 => pi + tiny,
            _ => -pi - tiny,
        };
    }
    if (ix as u32 | lx) == 0 {
        return if hy < 0 {
            -pi_o_2 - tiny
        } else {
            pi_o_2 + tiny
        };
    }
    if ix == 0x7FF00000 {
        if iy == 0x7FF00000 {
            return match m {
                0 => pi_o_4 + tiny,
                1 => -pi_o_4 - tiny,
                2 => 3.0 * pi_o_4 + tiny,
                _ => -3.0 * pi_o_4 - tiny,
            };
        }
        return match m {
            0 => 0.0,
            1 => -0.0,
            2 => pi + tiny,
            _ => -pi - tiny,
        };
    }
    if iy == 0x7FF00000 {
        return if hy < 0 {
            -pi_o_2 - tiny
        } else {
            pi_o_2 + tiny
        };
    }
    let k = (iy - ix) >> 20;
    let z = if k > 60 {
        m &= 1;
        pi_o_2 + 0.5 * pi_lo
    } else if hx < 0 && k < -60 {
        0.0
    } else {
        atan((y / x).abs())
    };
    match m {
        0 => z,
        1 => -z,
        2 => pi - (z - pi_lo),
        _ => (z - pi_lo) - pi,
    }
}

/// `Math.cos`.
pub fn cos(x: f64) -> f64 {
    let ix = high_word(x) & 0x7FFFFFFF;
    if ix <= 0x3FE921FB {
        kernel_cos(x, 0.0)
    } else if ix >= 0x7FF00000 {
        x - x
    } else {
        let mut y = [0.0; 2];
        let n = rem_pio2(x, &mut y);
        match n & 3 {
            0 => kernel_cos(y[0], y[1]),
            1 => -kernel_sin(y[0], y[1], 1),
            2 => -kernel_cos(y[0], y[1]),
            _ => kernel_sin(y[0], y[1], 1),
        }
    }
}

/// `Math.sin`.
pub fn sin(x: f64) -> f64 {
    let ix = high_word(x) & 0x7FFFFFFF;
    if ix <= 0x3FE921FB {
        kernel_sin(x, 0.0, 0)
    } else if ix >= 0x7FF00000 {
        x - x
    } else {
        let mut y = [0.0; 2];
        let n = rem_pio2(x, &mut y);
        match n & 3 {
            0 => kernel_sin(y[0], y[1], 1),
            1 => kernel_cos(y[0], y[1]),
            2 => -kernel_sin(y[0], y[1], 1),
            _ => -kernel_cos(y[0], y[1]),
        }
    }
}

/// `Math.tan`.
pub fn tan(x: f64) -> f64 {
    let ix = high_word(x) & 0x7FFFFFFF;
    if ix <= 0x3FE921FB {
        kernel_tan(x, 0.0, 1)
    } else if ix >= 0x7FF00000 {
        x - x
    } else {
        let mut y = [0.0; 2];
        let n = rem_pio2(x, &mut y);
        kernel_tan(y[0], y[1], 1 - ((n & 1) << 1))
    }
}

/// `Math.exp`.
pub fn exp(x: f64) -> f64 {
    let half = [0.5, -0.5];
    let o_threshold = 7.09782712893383973096e+02;
    let u_threshold = -7.45133219101941108420e+02;
    let ln2_hi = [6.93147180369123816490e-01, -6.93147180369123816490e-01];
    let ln2_lo = [1.90821492927058770002e-10, -1.90821492927058770002e-10];
    let invln2 = 1.44269504088896338700e+00;
    let p1 = 1.66666666666666019037e-01;
    let p2 = -2.77777777770155933842e-03;
    let p3 = 6.61375632143793436117e-05;
    let p4 = -1.65339022054652515390e-06;
    let p5 = 4.13813679705723846039e-08;
    let e = 2.718281828459045;
    let huge = 1.0e+300;
    let twom1000 = 9.33263618503218878990e-302;
    let two1023 = 8.988465674311579539e307;

    let mut x = x;
    let mut hi = 0.0;
    let mut lo = 0.0;
    let mut k: i32 = 0;
    let mut hx = (x.to_bits() >> 32) as u32;
    let xsb = ((hx >> 31) & 1) as usize;
    hx &= 0x7FFFFFFF;
    if hx >= 0x40862E42 {
        if hx >= 0x7FF00000 {
            let lx = low_word(x);
            return if ((hx & 0xFFFFF) | lx) != 0 {
                x + x
            } else if xsb == 0 {
                x
            } else {
                0.0
            };
        }
        if x > o_threshold {
            return f64::INFINITY;
        }
        if x < u_threshold {
            return twom1000 * twom1000;
        }
    }
    if hx > 0x3FD62E42 {
        if hx < 0x3FF0A2B2 {
            if x == 1.0 {
                return e;
            }
            hi = x - ln2_hi[xsb];
            lo = ln2_lo[xsb];
            k = 1 - xsb as i32 - xsb as i32;
        } else {
            k = (invln2 * x + half[xsb]) as i32;
            let t = k as f64;
            hi = x - t * ln2_hi[0];
            lo = t * ln2_lo[0];
        }
        x = hi - lo;
    } else if hx < 0x3E300000 {
        if huge + x > 1.0 {
            return 1.0 + x;
        }
    } else {
        k = 0;
    }
    let t = x * x;
    let twopk = if k >= -1021 {
        from_words(0x3FF00000u32.wrapping_add((k as u32) << 20), 0)
    } else {
        from_words(0x3FF00000u32.wrapping_add(((k + 1000) as u32) << 20), 0)
    };
    let c = x - t * (p1 + t * (p2 + t * (p3 + t * (p4 + t * p5))));
    if k == 0 {
        return 1.0 - ((x * c) / (c - 2.0) - x);
    }
    let y = 1.0 - ((lo - (x * c) / (2.0 - c)) - hi);
    if k >= -1021 {
        if k == 1024 {
            return y * 2.0 * two1023;
        }
        y * twopk
    } else {
        y * twopk * twom1000
    }
}

const LG: [f64; 7] = [
    6.666666666666735130e-01,
    3.999999999940941908e-01,
    2.857142874366239149e-01,
    2.222219843214978396e-01,
    1.818357216161805012e-01,
    1.531383769920937332e-01,
    1.479819860511658591e-01,
];
const LN2_HI: f64 = 6.93147180369123816490e-01;
const LN2_LO: f64 = 1.90821492927058770002e-10;
const TWO54: f64 = 1.80143985094819840000e+16;

/// `Math.log`.
pub fn log(x: f64) -> f64 {
    let mut x = x;
    let mut hx = high_word(x);
    let lx = low_word(x);
    let mut k: i32 = 0;
    if hx < 0x00100000 {
        if ((hx & 0x7FFFFFFF) as u32 | lx) == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= TWO54;
        hx = high_word(x);
    }
    if hx >= 0x7FF00000 {
        return x + x;
    }
    k += (hx >> 20) - 1023;
    hx &= 0x000FFFFF;
    let mut i = (hx + 0x95F64) & 0x100000;
    x = with_high(x, (hx | (i ^ 0x3FF00000)) as u32);
    k += i >> 20;
    let f = x - 1.0;
    if (0x000FFFFF & (2 + hx)) < 3 {
        if f == 0.0 {
            if k == 0 {
                return 0.0;
            }
            let dk = k as f64;
            return dk * LN2_HI + dk * LN2_LO;
        }
        let r = f * f * (0.5 - 0.33333333333333333 * f);
        if k == 0 {
            return f - r;
        }
        let dk = k as f64;
        return dk * LN2_HI - ((r - dk * LN2_LO) - f);
    }
    let s = f / (2.0 + f);
    let dk = k as f64;
    let z = s * s;
    i = hx - 0x6147A;
    let w = z * z;
    let j = 0x6B851 - hx;
    let t1 = w * (LG[1] + w * (LG[3] + w * LG[5]));
    let t2 = z * (LG[0] + w * (LG[2] + w * (LG[4] + w * LG[6])));
    i |= j;
    let r = t2 + t1;
    if i > 0 {
        let hfsq = 0.5 * f * f;
        if k == 0 {
            f - (hfsq - s * (hfsq + r))
        } else {
            dk * LN2_HI - ((hfsq - (s * (hfsq + r) + dk * LN2_LO)) - f)
        }
    } else if k == 0 {
        f - s * (f - r)
    } else {
        dk * LN2_HI - ((s * (f - r) - dk * LN2_LO) - f)
    }
}

/// `Math.log1p`.
pub fn log1p(x: f64) -> f64 {
    let lp = [
        6.666666666666735130e-01,
        3.999999999940941908e-01,
        2.857142874366239149e-01,
        2.222219843214978396e-01,
        1.818357216161805012e-01,
        1.531383769920937332e-01,
        1.479819860511658591e-01,
    ];
    let hx = high_word(x);
    let ax = hx & 0x7FFFFFFF;
    let mut k: i32 = 1;
    let mut f = 0.0;
    let mut hu: i32 = 0;
    let mut c = 0.0;
    if hx < 0x3FDA827A {
        if ax >= 0x3FF00000 {
            return if x == -1.0 {
                f64::NEG_INFINITY
            } else {
                f64::NAN
            };
        }
        if ax < 0x3E200000 {
            return if TWO54 + x > 0.0 && ax < 0x3C900000 {
                x
            } else {
                x - x * x * 0.5
            };
        }
        if hx > 0 || hx <= 0xBFD2BEC4u32 as i32 {
            k = 0;
            f = x;
            hu = 1;
        }
    }
    if hx >= 0x7FF00000 {
        return x + x;
    }
    if k != 0 {
        let mut u;
        if hx < 0x43400000 {
            u = 1.0 + x;
            hu = high_word(u);
            k = (hu >> 20) - 1023;
            c = if k > 0 { 1.0 - (u - x) } else { x - (u - 1.0) };
            c /= u;
        } else {
            u = x;
            hu = high_word(u);
            k = (hu >> 20) - 1023;
            c = 0.0;
        }
        hu &= 0x000FFFFF;
        if hu < 0x6A09E {
            u = with_high(u, (hu | 0x3FF00000) as u32);
        } else {
            k += 1;
            u = with_high(u, (hu | 0x3FE00000) as u32);
            hu = (0x00100000 - hu) >> 2;
        }
        f = u - 1.0;
    }
    let hfsq = 0.5 * f * f;
    let kf = k as f64;
    if hu == 0 {
        if f == 0.0 {
            if k == 0 {
                return 0.0;
            }
            c += kf * LN2_LO;
            return kf * LN2_HI + c;
        }
        let r = hfsq * (1.0 - 0.66666666666666666 * f);
        return if k == 0 {
            f - r
        } else {
            kf * LN2_HI - ((r - (kf * LN2_LO + c)) - f)
        };
    }
    let s = f / (2.0 + f);
    let z = s * s;
    let r = z
        * (lp[0] + z * (lp[1] + z * (lp[2] + z * (lp[3] + z * (lp[4] + z * (lp[5] + z * lp[6]))))));
    if k == 0 {
        f - (hfsq - s * (hfsq + r))
    } else {
        kf * LN2_HI - ((hfsq - (s * (hfsq + r) + (kf * LN2_LO + c))) - f)
    }
}

/// `k_log1p` helper of log2.
fn k_log1p(f: f64) -> f64 {
    let s = f / (2.0 + f);
    let z = s * s;
    let w = z * z;
    let t1 = w * (LG[1] + w * (LG[3] + w * LG[5]));
    let t2 = z * (LG[0] + w * (LG[2] + w * (LG[4] + w * LG[6])));
    let r = t2 + t1;
    let hfsq = 0.5 * f * f;
    s * (hfsq + r)
}

/// `Math.log2`.
pub fn log2(x: f64) -> f64 {
    let ivln2hi = 1.44269504072144627571e+00;
    let ivln2lo = 1.67517131648865118353e-10;
    let mut x = x;
    let mut hx = high_word(x);
    let lx = low_word(x);
    let mut k: i32 = 0;
    if hx < 0x00100000 {
        if ((hx & 0x7FFFFFFF) as u32 | lx) == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= TWO54;
        hx = high_word(x);
    }
    if hx >= 0x7FF00000 {
        return x + x;
    }
    if hx == 0x3FF00000 && lx == 0 {
        return 0.0;
    }
    k += (hx >> 20) - 1023;
    hx &= 0x000FFFFF;
    let i = (hx + 0x95F64) & 0x100000;
    x = with_high(x, (hx | (i ^ 0x3FF00000)) as u32);
    k += i >> 20;
    let y = k as f64;
    let f = x - 1.0;
    let hfsq = 0.5 * f * f;
    let r = k_log1p(f);
    let hi = with_low(f - hfsq, 0);
    let lo = (f - hi) - hfsq + r;
    let mut val_hi = hi * ivln2hi;
    let mut val_lo = (lo + hi) * ivln2lo + lo * ivln2hi;
    let w = y + val_hi;
    val_lo += (y - w) + val_hi;
    val_hi = w;
    val_lo + val_hi
}

/// `Math.log10`.
pub fn log10(x: f64) -> f64 {
    let ivln10 = 4.34294481903251816668e-01;
    let log10_2hi = 3.01029995663611771306e-01;
    let log10_2lo = 3.69423907715893078616e-13;
    let mut x = x;
    let mut hx = high_word(x);
    let mut lx = low_word(x);
    let mut k: i32 = 0;
    if hx < 0x00100000 {
        if ((hx & 0x7FFFFFFF) as u32 | lx) == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= TWO54;
        hx = high_word(x);
        lx = low_word(x);
    }
    if hx >= 0x7FF00000 {
        return x + x;
    }
    if hx == 0x3FF00000 && lx == 0 {
        return 0.0;
    }
    k += (hx >> 20) - 1023;
    let i = ((k as u32) >> 31) as i32;
    hx = (hx & 0x000FFFFF) | ((0x3FF - i) << 20);
    let y = (k + i) as f64;
    x = from_words(hx as u32, lx);
    let z = y * log10_2lo + ivln10 * log(x);
    z + y * log10_2hi
}

/// `Math.expm1`.
pub fn expm1(x: f64) -> f64 {
    let o_threshold = 7.09782712893383973096e+02;
    let ln2_hi = 6.93147180369123816490e-01;
    let ln2_lo = 1.90821492927058770002e-10;
    let invln2 = 1.44269504088896338700e+00;
    let q1 = -3.33333333333331316428e-02;
    let q2 = 1.58730158725481460165e-03;
    let q3 = -7.93650757867487942473e-05;
    let q4 = 4.00821782732936239552e-06;
    let q5 = -2.01099218183624371326e-07;
    let huge = 1.0e+300;
    let tiny = 1.0e-300;
    let one = 1.0;

    let mut x = x;
    let mut hx = (x.to_bits() >> 32) as u32;
    let xsb = hx & 0x80000000;
    hx &= 0x7FFFFFFF;
    if hx >= 0x4043687A {
        if hx >= 0x40862E42 {
            if hx >= 0x7FF00000 {
                let low = low_word(x);
                return if ((hx & 0xFFFFF) | low) != 0 {
                    x + x
                } else if xsb == 0 {
                    x
                } else {
                    -1.0
                };
            }
            if x > o_threshold {
                return f64::INFINITY;
            }
        }
        if xsb != 0 && x + tiny < 0.0 {
            return tiny - one;
        }
    }
    let k: i32;
    let mut c = 0.0;
    if hx > 0x3FD62E42 {
        let hi;
        let lo;
        if hx < 0x3FF0A2B2 {
            if xsb == 0 {
                hi = x - ln2_hi;
                lo = ln2_lo;
                k = 1;
            } else {
                hi = x + ln2_hi;
                lo = -ln2_lo;
                k = -1;
            }
        } else {
            k = (invln2 * x + if xsb == 0 { 0.5 } else { -0.5 }) as i32;
            let t = k as f64;
            hi = x - t * ln2_hi;
            lo = t * ln2_lo;
        }
        x = hi - lo;
        c = (hi - x) - lo;
    } else if hx < 0x3C900000 {
        let t = huge + x;
        return x - (t - (huge + x));
    } else {
        k = 0;
    }
    let hfx = 0.5 * x;
    let hxs = x * hfx;
    let r1 = one + hxs * (q1 + hxs * (q2 + hxs * (q3 + hxs * (q4 + hxs * q5))));
    let mut t = 3.0 - r1 * hfx;
    let mut e = hxs * ((r1 - t) / (6.0 - x * t));
    if k == 0 {
        return x - (x * e - hxs);
    }
    let twopk = from_words(0x3FF00000u32.wrapping_add((k as u32) << 20), 0);
    e = x * (e - c) - c;
    e -= hxs;
    if k == -1 {
        return 0.5 * (x - e) - 0.5;
    }
    if k == 1 {
        return if x < -0.25 {
            -2.0 * (e - (x + 0.5))
        } else {
            one + 2.0 * (x - e)
        };
    }
    if k <= -2 || k > 56 {
        let mut y = one - (e - x);
        if k == 1024 {
            y = y * 2.0 * 8.98846567431158e+307;
        } else {
            y *= twopk;
        }
        return y - one;
    }
    t = one;
    let mut y;
    if k < 20 {
        t = with_high(t, (0x3FF00000 - (0x200000 >> k)) as u32);
        y = t - (e - x);
        y *= twopk;
    } else {
        t = with_high(t, ((0x3FF - k) << 20) as u32);
        y = x - (e + t);
        y += one;
        y *= twopk;
    }
    y
}

/// `Math.cbrt`.
pub fn cbrt(x: f64) -> f64 {
    let b1: u32 = 715094163;
    let b2: u32 = 696219795;
    let p0 = 1.87595182427177009643;
    let p1 = -1.88497979543377169875;
    let p2 = 1.621429720105354466140;
    let p3 = -0.758397934778766047437;
    let p4 = 0.145996192886612446982;

    let hx0 = (x.to_bits() >> 32) as u32;
    let low = low_word(x);
    let sign = hx0 & 0x80000000;
    let hx = hx0 ^ sign;
    if hx >= 0x7FF00000 {
        return x + x;
    }
    let mut t;
    if hx < 0x00100000 {
        if (hx | low) == 0 {
            return x;
        }
        t = with_high(0.0, 0x43500000);
        t *= x;
        let high = (t.to_bits() >> 32) as u32;
        t = from_words(sign | ((high & 0x7FFFFFFF) / 3 + b2), 0);
    } else {
        t = from_words(sign | (hx / 3 + b1), 0);
    }
    let mut r = (t * t) * (t / x);
    t *= (p0 + r * (p1 + r * p2)) + ((r * r) * r) * (p3 + r * p4);
    let bits = (t.to_bits().wrapping_add(0x80000000)) & 0xFFFF_FFFF_C000_0000;
    t = f64::from_bits(bits);
    let s = t * t;
    r = x / s;
    let w = t + t;
    r = (r - t) / (w + r);
    t + t * r
}

/// `Math.cosh`.
pub fn cosh(x: f64) -> f64 {
    let overflow = 710.4758600739439;
    let ix = high_word(x) & 0x7FFFFFFF;
    if ix < 0x3FD62E43 {
        let t = expm1(x.abs());
        let w = 1.0 + t;
        if ix < 0x3C800000 {
            return w;
        }
        return 1.0 + (t * t) / (w + w);
    }
    if ix < 0x40360000 {
        let t = exp(x.abs());
        return 0.5 * t + 0.5 / t;
    }
    if ix < 0x40862E42 {
        return 0.5 * exp(x.abs());
    }
    if x.abs() <= overflow {
        let w = exp(0.5 * x.abs());
        let t = 0.5 * w;
        return t * w;
    }
    if ix >= 0x7FF00000 {
        return x * x;
    }
    f64::INFINITY
}

/// `Math.sinh`.
pub fn sinh(x: f64) -> f64 {
    let overflow = 710.4758600739439;
    let two_m28 = 3.725290298461914e-9;
    let log_maxd = 709.7822265625;
    let shuge = 1.0e307;
    let h = if x < 0.0 { -0.5 } else { 0.5 };
    let ax = x.abs();
    if ax < 22.0 {
        if ax < two_m28 {
            return x;
        }
        let t = expm1(ax);
        if ax < 1.0 {
            return h * (2.0 * t - t * t / (t + 1.0));
        }
        return h * (t + t / (t + 1.0));
    }
    if ax < log_maxd {
        return h * exp(ax);
    }
    if ax <= overflow {
        let w = exp(0.5 * ax);
        let t = h * w;
        return t * w;
    }
    x * shuge
}

/// `Math.tanh`.
pub fn tanh(x: f64) -> f64 {
    let tiny = 1.0e-300;
    let huge = 1.0e300;
    let jx = high_word(x);
    let ix = jx & 0x7FFFFFFF;
    if ix >= 0x7FF00000 {
        return if jx >= 0 {
            1.0 / x + 1.0
        } else {
            1.0 / x - 1.0
        };
    }
    let z;
    if ix < 0x40360000 {
        if ix < 0x3E300000 && huge + x > 1.0 {
            return x;
        }
        if ix >= 0x3FF00000 {
            let t = expm1(2.0 * x.abs());
            z = 1.0 - 2.0 / (t + 2.0);
        } else {
            let t = expm1(-2.0 * x.abs());
            z = -t / (t + 2.0);
        }
    } else {
        z = 1.0 - tiny;
    }
    if jx >= 0 { z } else { -z }
}

/// `Math.pow` (fdlibm `__ieee754_pow` as shipped in V8 12.4).
pub fn pow(x: f64, y: f64) -> f64 {
    let bp = [1.0, 1.5];
    let dp_h = [0.0, 5.84962487220764160156e-01];
    let dp_l = [0.0, 1.35003920212974897128e-08];
    let two53 = 9007199254740992.0;
    let huge = 1.0e300;
    let tiny = 1.0e-300;
    let l1 = 5.99999999999994648725e-01;
    let l2 = 4.28571428578550184252e-01;
    let l3 = 3.33333329818377432918e-01;
    let l4 = 2.72728123808534006489e-01;
    let l5 = 2.30660745775561754067e-01;
    let l6 = 2.06975017800338417784e-01;
    let p1 = 1.66666666666666019037e-01;
    let p2 = -2.77777777770155933842e-03;
    let p3 = 6.61375632143793436117e-05;
    let p4 = -1.65339022054652515390e-06;
    let p5 = 4.13813679705723846039e-08;
    let lg2 = 6.93147180559945286227e-01;
    let lg2_h = 6.93147182464599609375e-01;
    let lg2_l = -1.90465429995776804525e-09;
    let ovt = 8.0085662595372944372e-0017;
    let cp = 9.61796693925975554329e-01;
    let cp_h = 9.61796700954437255859e-01;
    let cp_l = -7.02846165095275826516e-09;
    let ivln2 = 1.44269504088896338700e+00;
    let ivln2_h = 1.44269502162933349609e+00;
    let ivln2_l = 1.92596299112661746887e-08;

    let hx = high_word(x);
    let lx = low_word(x);
    let hy = high_word(y);
    let ly = low_word(y);
    let mut ix = hx & 0x7fffffff;
    let iy = hy & 0x7fffffff;

    if (iy as u32 | ly) == 0 {
        return 1.0;
    }
    if ix > 0x7ff00000
        || (ix == 0x7ff00000 && lx != 0)
        || iy > 0x7ff00000
        || (iy == 0x7ff00000 && ly != 0)
    {
        return x + y;
    }

    let mut yisint = 0;
    if hx < 0 {
        if iy >= 0x43400000 {
            yisint = 2;
        } else if iy >= 0x3ff00000 {
            let k = (iy >> 20) - 0x3ff;
            if k > 20 {
                let j = (ly >> (52 - k)) as i32;
                if ((j as u32) << (52 - k)) as i32 == ly as i32 {
                    yisint = 2 - (j & 1);
                }
            } else if ly == 0 {
                let j = iy >> (20 - k);
                if (j << (20 - k)) == iy {
                    yisint = 2 - (j & 1);
                }
            }
        }
    }

    if ly == 0 {
        if iy == 0x7ff00000 {
            return if ((ix - 0x3ff00000) as u32 | lx) == 0 {
                y - y
            } else if ix >= 0x3ff00000 {
                if hy >= 0 { y } else { 0.0 }
            } else if hy < 0 {
                -y
            } else {
                0.0
            };
        }
        if iy == 0x3ff00000 {
            return if hy < 0 { 1.0 / x } else { x };
        }
        if hy == 0x40000000 {
            return x * x;
        }
        if hy == 0x3fe00000 && hx >= 0 {
            return x.sqrt();
        }
    }

    let mut ax = x.abs();
    if lx == 0 && (ix == 0x7ff00000 || ix == 0 || ix == 0x3ff00000) {
        let mut z = ax;
        if hy < 0 {
            z = 1.0 / z;
        }
        if hx < 0 {
            if ((ix - 0x3ff00000) | yisint) == 0 {
                z = f64::NAN;
            } else if yisint == 1 {
                z = -z;
            }
        }
        return z;
    }

    let mut n = (hx >> 31) + 1;
    if (n | yisint) == 0 {
        return f64::NAN;
    }
    let mut s = 1.0;
    if (n | (yisint - 1)) == 0 {
        s = -1.0;
    }

    let t1;
    let t2;
    if iy > 0x41e00000 {
        if iy > 0x43f00000 {
            if ix <= 0x3fefffff {
                return if hy < 0 { f64::INFINITY } else { 0.0 };
            }
            if ix >= 0x3ff00000 {
                return if hy > 0 { f64::INFINITY } else { 0.0 };
            }
        }
        if ix < 0x3fefffff {
            return if hy < 0 {
                s * huge * huge
            } else {
                s * tiny * tiny
            };
        }
        if ix > 0x3ff00000 {
            return if hy > 0 {
                s * huge * huge
            } else {
                s * tiny * tiny
            };
        }
        let t = ax - 1.0;
        let w = (t * t) * (0.5 - t * (0.3333333333333333333333 - t * 0.25));
        let u = ivln2_h * t;
        let v = t * ivln2_l - w * ivln2;
        t1 = with_low(u + v, 0);
        t2 = v - (t1 - u);
    } else {
        n = 0;
        if ix < 0x00100000 {
            ax *= two53;
            n -= 53;
            ix = high_word(ax);
        }
        n += (ix >> 20) - 0x3ff;
        let j = ix & 0x000fffff;
        ix = j | 0x3ff00000;
        let k: usize;
        if j <= 0x3988E {
            k = 0;
        } else if j < 0xBB67A {
            k = 1;
        } else {
            k = 0;
            n += 1;
            ix -= 0x00100000;
        }
        ax = with_high(ax, ix as u32);

        let u = ax - bp[k];
        let v = 1.0 / (ax + bp[k]);
        let ss = u * v;
        let s_h = with_low(ss, 0);
        let t_h = with_high(
            0.0,
            (((ix >> 1) | 0x20000000) + 0x00080000 + ((k as i32) << 18)) as u32,
        );
        let t_l = ax - (t_h - bp[k]);
        let s_l = v * ((u - s_h * t_h) - s_h * t_l);
        let mut s2 = ss * ss;
        let mut r = s2 * s2 * (l1 + s2 * (l2 + s2 * (l3 + s2 * (l4 + s2 * (l5 + s2 * l6)))));
        r += s_l * (s_h + ss);
        s2 = s_h * s_h;
        let t_h = with_low(3.0 + s2 + r, 0);
        let t_l = r - ((t_h - 3.0) - s2);
        let u = s_h * t_h;
        let v = s_l * t_h + t_l * ss;
        let p_h = with_low(u + v, 0);
        let p_l = v - (p_h - u);
        let z_h = cp_h * p_h;
        let z_l = cp_l * p_h + p_l * cp + dp_l[k];
        let t = n as f64;
        t1 = with_low(((z_h + z_l) + dp_h[k]) + t, 0);
        t2 = z_l - (((t1 - t) - dp_h[k]) - z_h);
    }

    let y1 = with_low(y, 0);
    let p_l = (y - y1) * t1 + y * t2;
    let mut p_h = y1 * t1;
    let mut z = p_l + p_h;
    let j = high_word(z);
    let i = low_word(z);
    if j >= 0x40900000 {
        if ((j - 0x40900000) as u32 | i) != 0 {
            return s * huge * huge;
        } else if p_l + ovt > z - p_h {
            return s * huge * huge;
        }
    } else if (j & 0x7fffffff) >= 0x4090cc00 {
        if (j.wrapping_sub(0xc090cc00u32 as i32) as u32 | i) != 0 {
            return s * tiny * tiny;
        } else if p_l <= z - p_h {
            return s * tiny * tiny;
        }
    }

    let i = j & 0x7fffffff;
    let mut k = (i >> 20) - 0x3ff;
    n = 0;
    if i > 0x3fe00000 {
        n = j.wrapping_add(0x00100000 >> (k + 1));
        k = ((n & 0x7fffffff) >> 20) - 0x3ff;
        let t = with_high(0.0, (n & !(0x000fffff >> k)) as u32);
        n = ((n & 0x000fffff) | 0x00100000) >> (20 - k);
        if j < 0 {
            n = -n;
        }
        p_h -= t;
    }
    let mut t = with_low(p_l + p_h, 0);
    let u = t * lg2_h;
    let v = (p_l - (t - p_h)) * lg2 + t * lg2_l;
    z = u + v;
    let w = v - (z - u);
    t = z * z;
    let t1 = z - t * (p1 + t * (p2 + t * (p3 + t * (p4 + t * p5))));
    let r = (z * t1) / ((t1 - 2.0) - (w + z * w));
    z = 1.0 - (r - z);
    let mut j = high_word(z);
    j = j.wrapping_add(((n as u32) << 20) as i32);
    if (j >> 20) <= 0 {
        z = scalbn(z, n);
    } else {
        let tmp = high_word(z);
        z = with_high(z, tmp.wrapping_add(((n as u32) << 20) as i32) as u32);
    }
    s * z
}

/// `Math.hypot` over any number of arguments (V8 builtin: Kahan sum normalised by the maximum).
pub fn hypot(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut one_is_nan = false;
    let mut max = 0.0;
    for &v in values {
        if v.is_nan() {
            one_is_nan = true;
        } else if v.abs() > max {
            max = v.abs();
        }
    }
    if max == f64::INFINITY {
        return f64::INFINITY;
    } else if one_is_nan {
        return f64::NAN;
    } else if max == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut compensation = 0.0;
    for &v in values {
        // A NaN slot is zero-filled in V8, but NaN already returned above.
        let n = v.abs() / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    f64::sqrt(sum) * max
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn parse(s: &str) -> f64 {
        f64::from_bits(u64::from_str_radix(s, 16).unwrap())
    }

    fn same(a: f64, b: f64) -> bool {
        (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
    }

    #[test]
    fn matches_node_golden_values() {
        let golden: Value =
            serde_json::from_str(include_str!("../../Tests/Golden/V8Math.json")).unwrap();
        let unary: [(&str, fn(f64) -> f64); 15] = [
            ("exp", exp),
            ("expm1", expm1),
            ("log", log),
            ("log2", log2),
            ("log10", log10),
            ("log1p", log1p),
            ("atan", atan),
            ("sin", sin),
            ("cos", cos),
            ("tan", tan),
            ("asin", asin),
            ("acos", acos),
            ("sinh", sinh),
            ("cosh", cosh),
            ("tanh", tanh),
        ];
        let mut failures = Vec::new();
        let mut check = |name: &str, args: &[f64], got: f64, want: f64| {
            if !same(got, want) {
                failures.push(format!(
                    "{name}({args:?}) = {got:?} ({:016x}), want {want:?} ({:016x})",
                    got.to_bits(),
                    want.to_bits()
                ));
            }
        };
        for (name, f) in unary {
            for row in golden[name].as_array().unwrap() {
                let (x, want) = (
                    parse(row[0].as_str().unwrap()),
                    parse(row[1].as_str().unwrap()),
                );
                check(name, &[x], f(x), want);
            }
        }
        for row in golden["cbrt"].as_array().unwrap() {
            let (x, want) = (
                parse(row[0].as_str().unwrap()),
                parse(row[1].as_str().unwrap()),
            );
            check("cbrt", &[x], cbrt(x), want);
        }
        let binary: [(&str, fn(f64, f64) -> f64); 3] = [
            ("atan2", atan2),
            ("pow", pow),
            ("hypot", |x, y| hypot(&[x, y])),
        ];
        for (name, f) in binary {
            for row in golden[name].as_array().unwrap() {
                let x = parse(row[0].as_str().unwrap());
                let y = parse(row[1].as_str().unwrap());
                check(name, &[x, y], f(x, y), parse(row[2].as_str().unwrap()));
            }
        }
        assert!(
            failures.is_empty(),
            "{} mismatches, first: {:#?}",
            failures.len(),
            &failures[..failures.len().min(15)]
        );
    }
}
